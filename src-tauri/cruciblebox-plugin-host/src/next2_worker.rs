//! Generic API 6 JS execution. Stdout contains only bounded wire 4 frames.
use cruciblebox_next2_protocol as protocol;
use cruciblebox_next2_runtime::transport;
use rquickjs::{CatchResultExt, Context, Function, Runtime, Value};
use serde_json::{json, Value as Json};
use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

fn evaluate(context: &Context, script: &str) -> Result<Json, String> {
    context.with(|ctx| {
        let result: Value = ctx.eval(script).catch(&ctx).map_err(|e| e.to_string())?;
        let result = if result.is_promise() {
            rquickjs::Promise::from_value(result)
                .map_err(|e| e.to_string())?
                .finish::<Value>()
                .catch(&ctx)
                .map_err(|e| e.to_string())?
        } else {
            result
        };
        let raw = ctx
            .json_stringify(result)
            .map_err(|e| e.to_string())?
            .map(|v| v.to_string())
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "null".into());
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    })
}
pub fn run(directory: PathBuf, entry: &str) -> Result<(), String> {
    let manifest = protocol::validate_manifest(
        &std::fs::read_to_string(directory.join("plugin.json")).map_err(|e| e.to_string())?,
    )?;
    let Some(protocol::Backend::JsBackend(backend)) = manifest.backend else {
        return Err("JS_BACKEND_REQUIRED".into());
    };
    if backend.entry != entry {
        return Err("ENTRY_MISMATCH".into());
    }
    let output = Arc::new(Mutex::new(io::stdout()));
    transport::write_frame(
        &mut *output.lock().map_err(|_| "OUTPUT_UNAVAILABLE")?,
        &json!({"kind":"hello","wireVersion":4,"sdkApiVersion":6}),
    )
    .map_err(|e| e.to_string())?;
    let mut input = io::stdin();
    let init = transport::read_frame(&mut input)?.ok_or("MISSING_INITIALIZATION")?;
    if init["kind"] != "initialize" || init["pluginId"] != manifest.id {
        return Err("INITIALIZATION_DENIED".into());
    }
    let token = init["session"]
        .as_str()
        .ok_or("INVALID_SESSION")?
        .to_owned();
    let cancelled = Arc::new(AtomicBool::new(false));
    let task = Arc::new(Mutex::new(None::<String>));
    let (tx, rx) = mpsc::sync_channel(32);
    let reader_cancelled = cancelled.clone();
    let reader_task = task.clone();
    // A separate reader receives cancellation while QuickJS is evaluating a task.
    // Process shutdown closes the pipe; the OS reclaims this reader at exit.
    std::thread::Builder::new()
        .name("next2-js-input".into())
        .spawn(move || loop {
            let frame = transport::read_frame(&mut input)
                .and_then(|frame| frame.ok_or("HOST_DISCONNECTED".into()));
            if let Ok(frame) = &frame {
                if frame["kind"] == "execute" {
                    if let Ok(mut task) = reader_task.lock() {
                        *task = frame["taskId"].as_str().map(str::to_owned);
                    }
                    reader_cancelled.store(false, Ordering::SeqCst);
                }
                if frame["kind"] == "cancel" {
                    if reader_task
                        .lock()
                        .ok()
                        .and_then(|task| task.clone())
                        .as_deref()
                        == frame["taskId"].as_str()
                    {
                        reader_cancelled.store(true, Ordering::SeqCst);
                    }
                    continue;
                }
                if frame["kind"] == "stop" {
                    reader_cancelled.store(true, Ordering::SeqCst);
                }
            }
            let failed = frame.is_err();
            if tx.send(frame).is_err() || failed {
                break;
            }
        })
        .map_err(|e| e.to_string())?;
    let input = Arc::new(Mutex::new(rx));
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(64 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    let deadline = Arc::new(Mutex::new(Some(Instant::now() + Duration::from_secs(10))));
    let interrupt = deadline.clone();
    let interrupted = cancelled.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        interrupted.load(Ordering::SeqCst)
            || interrupt.lock().map_or(true, |deadline| {
                deadline.is_some_and(|time| Instant::now() >= time)
            })
    })));
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    context.with(|ctx| -> Result<(), String> {
        super::install_polyfills(&ctx).map_err(|e| e.to_string())?;
        super::install_cjs_host_functions(&ctx, &directory).map_err(|e| e.to_string())?;
        ctx.eval::<(), _>(super::loader::LOADER_JS).map_err(|e| e.to_string())?;
        let input = input.clone(); let output = output.clone(); let token = token.clone();
        ctx.globals().set("__nextExchange", Function::new(ctx.clone(), move |raw: String| -> Result<String, rquickjs::Error> {
            let result = (|| -> Result<String, String> {
                let request = protocol::validate_request(&raw)?;
                if request.session != token { return Err("SESSION_DENIED".into()); }
                transport::write_frame(&mut *output.lock().map_err(|_| "OUTPUT_UNAVAILABLE")?, &json!({"kind":"capability","request":request})).map_err(|e| e.to_string())?;
                let frame = input.lock().map_err(|_| "INPUT_UNAVAILABLE")?.recv().map_err(|e| e.to_string())??;
                if frame["kind"] != "capability-result" { return Err("UNEXPECTED_CAPABILITY_FRAME".into()); }
                protocol::validate_response(&frame["response"].to_string(), &request.request_id)?;
                Ok(frame["response"].to_string())
            })();
            result.map_err(|e| rquickjs::Error::new_from_js_message("host", "response", e))
        }).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let flag = cancelled.clone();
        ctx.globals().set("__nextCancelled", Function::new(ctx.clone(), move || flag.load(Ordering::SeqCst)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        ctx.globals().set("__nextUtf8", Function::new(ctx.clone(), |value: String| value.into_bytes()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        ctx.eval::<(), _>("globalThis.TextEncoder=class TextEncoder {encode(value=''){return Uint8Array.from(__nextUtf8(String(value)));}};").map_err(|e| e.to_string())?;
        Ok(())
    })?;
    let entry = directory.join(entry);
    let entry_raw = serde_json::to_string(&entry.to_string_lossy().replace('\\', "/"))
        .map_err(|e| e.to_string())?;
    let parent_raw = serde_json::to_string(
        &entry
            .parent()
            .ok_or("INVALID_ENTRY")?
            .to_string_lossy()
            .replace('\\', "/"),
    )
    .map_err(|e| e.to_string())?;
    let session_raw = serde_json::to_string(&token).map_err(|e| e.to_string())?;
    evaluate(&context, &format!("__cjsCurrentDir={parent_raw};globalThis.__nextPlugin=require({entry_raw});globalThis.__nextContext={{...{},session:{session_raw},exchange:async request=>JSON.parse(__nextExchange(JSON.stringify(request))),isCancelled:()=>__nextCancelled()}};Promise.resolve(__nextPlugin.activate(__nextContext)).then(api=>{{globalThis.__nextApi=api;return null;}})", init))?;
    transport::write_frame(
        &mut *output.lock().map_err(|_| "OUTPUT_UNAVAILABLE")?,
        &json!({"kind":"ready","wireVersion":4}),
    )
    .map_err(|e| e.to_string())?;
    loop {
        let frame = input
            .lock()
            .map_err(|_| "INPUT_UNAVAILABLE")?
            .recv()
            .map_err(|e| e.to_string())??;
        if frame["kind"] == "stop" {
            break;
        }
        if frame["kind"] != "execute" || frame["request"]["session"] != token {
            return Err("EXECUTION_DENIED".into());
        }
        *deadline.lock().map_err(|_| "DEADLINE_UNAVAILABLE")? = if frame["taskId"].is_string() {
            None
        } else {
            Some(Instant::now() + Duration::from_secs(30))
        };
        let request = &frame["request"];
        let name = request["method"].to_string();
        let params = request["params"].to_string();
        let execution = json!({"callerId":frame["callerId"],"taskId":frame["taskId"]});
        let result = evaluate(&context, &format!("Promise.resolve().then(()=>{{const name={name};if(!Object.hasOwn(__nextApi,name)||typeof __nextApi[name]!== 'function')throw Object.assign(Error('Unknown backend method'),{{code:'METHOD_UNAVAILABLE'}});return __nextApi[name]({params},{{...{execution},isCancelled:__nextContext.isCancelled}});}}).then(result=>({{ok:true,result:result===undefined?null:result}}),error=>({{ok:false,error:{{code:typeof error.code==='string'?error.code:'JS_EXECUTION_FAILED',message:String(error.message||error)}}}}))"));
        let response = match result {
            Ok(value) => {
                let mut value = value;
                value["wireVersion"] = json!(4);
                value["requestId"] = request["requestId"].clone();
                value
            }
            Err(message) => {
                json!({"wireVersion":4,"requestId":request["requestId"],"ok":false,"error":{"code":if cancelled.load(Ordering::SeqCst) {"CANCELLED"} else {"JS_EXECUTION_FAILED"},"message":message}})
            }
        };
        protocol::validate_response(
            &response.to_string(),
            request["requestId"].as_str().ok_or("INVALID_REQUEST")?,
        )?;
        transport::write_frame(
            &mut *output.lock().map_err(|_| "OUTPUT_UNAVAILABLE")?,
            &json!({"kind":"reply","response":response}),
        )
        .map_err(|e| e.to_string())?;
    }
    cancelled.store(false, Ordering::SeqCst);
    *deadline.lock().map_err(|_| "DEADLINE_UNAVAILABLE")? =
        Some(Instant::now() + Duration::from_secs(5));
    evaluate(
        &context,
        "typeof __nextPlugin.deactivate === 'function' ? __nextPlugin.deactivate() : null",
    )?;
    Ok(())
}
