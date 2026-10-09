//! Next backend execution. Own bounded wire 3, no legacy ctx or frame queue.
use cruciblebox_next_protocol::{self as protocol, transport};
use rquickjs::{Context, Function, Runtime, Value};
use serde_json::{json, Value as Json};
use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub fn run(directory: PathBuf, entry: PathBuf, token: String) -> Result<(), String> {
    let manifest = protocol::validate_manifest(
        &std::fs::read_to_string(directory.join("plugin.json")).map_err(|e| e.to_string())?,
    )
    .map_err(str::to_owned)?;
    if manifest.backend.as_deref() != Some("dist/main.js")
        || entry != directory.join("dist/main.js")
    {
        return Err("INVALID_MANIFEST".into());
    }
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(64 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    let deadline = Arc::new(Mutex::new(
        Instant::now() + Duration::from_millis(protocol::BACKEND_TIMEOUT_MS),
    ));
    let interrupt = deadline.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        interrupt
            .lock()
            .map_or(true, |deadline| Instant::now() >= *deadline)
    })));
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    let input = Arc::new(Mutex::new(io::stdin()));
    let output = Arc::new(Mutex::new(io::stdout()));
    context.with(|ctx| -> Result<(),String> {
        super::install_polyfills(&ctx).map_err(|e|e.to_string())?;
        super::install_cjs_host_functions(&ctx,&directory).map_err(|e|e.to_string())?;
        ctx.eval::<(),_>(super::loader::LOADER_JS).map_err(|e|e.to_string())?;
        let input=input.clone();let output=output.clone();let expected=token.clone();
        let exchange=Function::new(ctx.clone(),move |raw:String| -> Result<String,rquickjs::Error> {
            let result=(|| -> Result<String,String> {
                let request=protocol::validate_typed_request(&raw).map_err(str::to_owned)?;
                if request.session!=expected {return Err("SESSION_DENIED".into());}
                let id=request.request_id.clone();
                transport::write_frame(&mut *output.lock().map_err(|_|"INTERNAL_ERROR")?,&json!({"kind":"capability","request":request}))?;
                let bytes=transport::read_frame(&mut *input.lock().map_err(|_|"INTERNAL_ERROR")?).map_err(|e|e.to_string())?.ok_or("SESSION_DENIED")?;
                let raw=std::str::from_utf8(&bytes).map_err(|_|"INVALID_RESPONSE")?;
                let frame=transport::validate_frame(raw,&expected).map_err(str::to_owned)?;
                if frame["kind"]!="capability-result" {return Err("INVALID_RESPONSE".into());}
                protocol::validate_response(&frame["response"].to_string(),&id).map_err(str::to_owned)?;
                Ok(frame["response"].to_string())
            })();
            result.map_err(|e|rquickjs::Error::new_from_js_message("host", "response", e))
        }).map_err(|e|e.to_string())?;
        ctx.globals().set("__nextExchange",exchange).map_err(|e|e.to_string())?;
        let entry_json=serde_json::to_string(&entry.to_string_lossy().replace('\\',"/")).map_err(|e|e.to_string())?;
        let directory_json=serde_json::to_string(&entry.parent().ok_or("INVALID_MANIFEST")?.to_string_lossy().replace('\\',"/")).map_err(|e|e.to_string())?;
        let token_json=serde_json::to_string(&token).map_err(|e|e.to_string())?;
        ctx.eval::<(),_>(format!("__cjsCurrentDir={directory_json}; globalThis.__nextPlugin=require({entry_json}); globalThis.__nextContext={{session:{token_json},exchange:async function(request){{return JSON.parse(__nextExchange(JSON.stringify(request)));}}}};")).map_err(|e|e.to_string())?;
        Ok(())
    })?;
    let mut active = false;
    loop {
        let bytes = {
            let mut reader = input.lock().map_err(|_| "INTERNAL_ERROR")?;
            transport::read_frame(&mut *reader).map_err(|e| e.to_string())?
        };
        let Some(bytes) = bytes else {
            break;
        };
        let frame = transport::validate_frame(
            std::str::from_utf8(&bytes).map_err(|_| "INVALID_REQUEST")?,
            &token,
        )
        .map_err(str::to_owned)?;
        if frame["kind"] != "control" {
            return Err("INVALID_REQUEST".into());
        }
        *deadline.lock().map_err(|_| "INTERNAL_ERROR")? =
            Instant::now() + Duration::from_millis(protocol::BACKEND_TIMEOUT_MS);
        let method = frame["method"].as_str().ok_or("INVALID_REQUEST")?;
        let id = frame["requestId"].as_str().ok_or("INVALID_REQUEST")?;
        let script=match method {
            "activate" if !active => "Promise.resolve(__nextPlugin.activate(__nextContext)).then(api => { globalThis.__nextApi=api; return null; });".to_string(),
            "call" if active => {
                let name=serde_json::to_string(&frame["params"]["method"]).map_err(|e|e.to_string())?;
                let args=frame["params"]["args"].to_string();
                format!("(function(){{const name={name};if(!Object.prototype.hasOwnProperty.call(__nextApi,name)||typeof __nextApi[name]!== 'function')throw Error('Unknown backend method');return __nextApi[name](...{args});}})()")
            }
            "dispose" => "if(typeof __nextPlugin.deactivate==='function')__nextPlugin.deactivate(); else null;".to_string(),
            _ => return Err("SESSION_DENIED".into()),
        };
        let result = context.with(|ctx| -> Result<Json, String> {
            let result: Value = ctx.eval(script.as_str()).map_err(|e| e.to_string())?;
            let value = if result.is_promise() {
                rquickjs::Promise::from_value(result)
                    .map_err(|e| e.to_string())?
                    .finish::<Value>()
                    .map_err(|e| e.to_string())?
            } else {
                result
            };
            let text = ctx
                .json_stringify(value)
                .map_err(|e| e.to_string())?
                .map(|v| v.to_string())
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or("null".into());
            serde_json::from_str(&text).map_err(|e| e.to_string())
        });
        let result = if method == "activate" && result.is_ok() {
            Ok(
                json!({"contractSha256":protocol::CONTRACT_SHA256,"sdkApiVersion":5,"wireVersion":3}),
            )
        } else {
            result
        };
        let mut response = match result {
            Ok(value) => json!({"wireVersion":3,"requestId":id,"ok":true,"result":value}),
            Err(_) => {
                json!({"wireVersion":3,"requestId":id,"ok":false,"error":{"code":"INTERNAL_ERROR","message":"Backend execution failed"}})
            }
        };
        if protocol::validate_response(&response.to_string(), id).is_err()
            || protocol::validate_payload(&json!({"kind":"result","response":response}).to_string())
                .is_err()
        {
            response = json!({"wireVersion":3,"requestId":id,"ok":false,"error":{"code":"BUDGET_EXCEEDED","message":"BUDGET_EXCEEDED"}});
        }
        transport::write_frame(
            &mut *output.lock().map_err(|_| "INTERNAL_ERROR")?,
            &json!({"kind":"result","response":response}),
        )?;
        if method == "activate" && response["ok"] == true {
            active = true;
        }
        if method == "dispose" {
            break;
        }
    }
    Ok(())
}
