// CrucibleBox 插件 backend sidecar 主循环（1.8.2）
// 生命周期（对等 PluginProcessEntry）：
//   argv: [exe, pluginDir, mainEntry(相对), apiVersion, token]
//   主循环：读帧 → 校验 worker 请求信封 → 分发
//     lifecycle.initialize → 加载 CJS（真实 loader）→ 构建 ctx → activate
//     lifecycle.dispose    → deactivate → 退出
//     plugin.message       → onMessage → 回响应
//   JS 内 ctx 方法经 __hostRequest 同步转发宿主（见 ctx.rs）
//
// 架构：Engine 持有 JsRuntime + JsContext（无 lifetime 贯穿）；plugin 对象与 ctx
// 存 JS 全局（__cbPlugin / __cbCtx），跨帧复用同一 JS 上下文。

mod ctx;
mod envelope;
mod frame;
mod loader;

use rquickjs::{Context as JsContext, Function, Object, Runtime as JsRuntime, Value};
use serde_json::Value as Json;
use std::cell::RefCell;
#[cfg(unix)]
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

struct Engine {
    // rt 必须被持有：JsContext 的生命周期依赖 runtime 存活
    #[allow(dead_code)]
    rt: JsRuntime,
    ctx: JsContext,
}

struct Worker {
    plugin_dir: PathBuf,
    main_entry: PathBuf,
    token: String,
    engine: Option<Rc<RefCell<Engine>>>,
    disposed: bool,
}

type Err2 = (String, Option<String>);

fn secure_random_u32() -> Result<u32, String> {
    let mut bytes = [0u8; 4];
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        extern "system" {
            fn BCryptGenRandom(
                h_algorithm: *mut std::ffi::c_void,
                buffer: *mut u8,
                buffer_len: u32,
                flags: u32,
            ) -> i32;
        }
        // BCRYPT_USE_SYSTEM_PREFERRED_RNG
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                0x00000002,
            )
        };
        if status != 0 {
            return Err(format!("BCryptGenRandom failed with status {status:#x}"));
        }
    }
    #[cfg(unix)]
    {
        std::fs::File::open("/dev/urandom")
            .map_err(|error| format!("open /dev/urandom failed: {error}"))?
            .read_exact(&mut bytes)
            .map_err(|error| format!("read /dev/urandom failed: {error}"))?;
    }
    #[cfg(not(any(windows, unix)))]
    {
        return Err("secure random source is not supported on this platform".into());
    }
    Ok(u32::from_le_bytes(bytes))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let plugin_dir = args
        .get(1)
        .map(PathBuf::from)
        .expect("usage: plugin-host <pluginDir> <mainEntry> <apiVersion> <token>");
    let main_entry_rel = args.get(2).expect("missing mainEntry");
    let api_version = args.get(3).map(|s| s.as_str()).unwrap_or("");
    let token = args.get(4).expect("missing token").clone();
    if !envelope::valid_token(&token) {
        eprintln!("[host] invalid token format");
        std::process::exit(1);
    }
    // M4: apiVersion 闸门（v2 才走本协议）
    if api_version != "2" {
        eprintln!("[host] unsupported apiVersion: {api_version}");
        std::process::exit(1);
    }
    let main_entry = plugin_dir.join(main_entry_rel);
    if !main_entry.is_file() {
        eprintln!("[host] main entry not found: {}", main_entry.display());
        std::process::exit(1);
    }

    ctx::init(token.clone());

    let mut worker = Worker {
        plugin_dir,
        main_entry,
        token,
        engine: None,
        disposed: false,
    };

    let queue = ctx::FrameQueue::global();
    while let Some(frame_bytes) = queue.take_worker_frame() {
        let envelope: Json = match serde_json::from_slice(&frame_bytes) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[host] bad envelope json: {e}");
                continue;
            }
        };
        let (request_id, method, params) =
            match envelope::validate_worker_request(&envelope, &worker.token) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[host] envelope rejected: {e}");
                    continue;
                }
            };
        let response = match dispatch(&mut worker, &method, &params) {
            Ok(result) => envelope::make_response(&worker.token, &request_id, result),
            Err((code, msg)) => {
                let message = msg.unwrap_or_else(|| "worker error".into());
                envelope::make_error(&worker.token, &request_id, &code, &message)
            }
        };
        let bytes = serde_json::to_vec(&response).unwrap_or_else(|_| br#"{}"#.to_vec());
        if frame::write_frame(&mut std::io::stdout(), &bytes).is_err() {
            break;
        }
        if worker.disposed {
            break;
        }
    }
}

fn dispatch(worker: &mut Worker, method: &str, params: &Json) -> Result<Json, Err2> {
    match method {
        "lifecycle.initialize" => initialize(worker, params),
        "lifecycle.dispose" => dispose(worker),
        "plugin.message" => plugin_message(worker, params),
        "host.event" => host_event(worker, params),
        _ => Err(("NOT_ALLOWED".into(), Some("unknown worker method".into()))),
    }
}

fn initialize(worker: &mut Worker, params: &Json) -> Result<Json, Err2> {
    if worker.engine.is_some() {
        return Err(("INVALID_MESSAGE".into(), Some("already initialized".into())));
    }
    let config = params.get("config").cloned().unwrap_or(Json::Null);
    let id = params
        .get("pluginId")
        .and_then(Json::as_str)
        .unwrap_or("plugin")
        .to_string();

    let rt = JsRuntime::new().map_err(js_err)?;
    let ctx = JsContext::full(&rt).map_err(js_err)?;
    let engine = Rc::new(RefCell::new(Engine { rt, ctx }));

    {
        let engine_ref = engine.clone();
        let eng = engine_ref.borrow();
        let init_result = eng.ctx.with(|ctx| -> Result<(), (String, Option<String>)> {
            install_polyfills(&ctx)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("polyfills: {e}"))))?;
            ctx::register_host_request(&ctx)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("hostRequest: {e}"))))?;
            ctx.eval::<(), _>(ctx::CTX_JS)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("CTX_JS: {e}"))))?;
            ctx.eval::<(), _>(loader::LOADER_JS)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("LOADER_JS: {e}"))))?;
            install_cjs_host_functions(&ctx, &worker.plugin_dir)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("cjs fns: {e}"))))?;

            // CJS 引导：以入口文件为模块上下文执行，插件对象存 JS 全局
            let entry = worker.main_entry.to_string_lossy().replace('\\', "/");
            let boot = format!(
                r#"
                var entryAbs = "{entry}";
                var entryDir = entryAbs.substring(0, entryAbs.lastIndexOf('/'));
                __cjsCurrentDir = entryDir;
                var m = require(entryAbs);
                var plugin = (m && m.__esModule && m.default) ? m.default
                           : (m && m.default ? m.default : m);
                globalThis.__cbPlugin = plugin;
                plugin;
                "#
            );
            let plugin_val: Value = ctx
                .eval(boot.as_str())
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("boot: {e}"))))?;
            assert!(
                !plugin_val.is_undefined() && !plugin_val.is_null(),
                "plugin not resolved"
            );

            // 构建 ctx 存 JS 全局
            let id_json = serde_json::to_string(&id)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("id json: {e}"))))?;
            let config_json = serde_json::to_string(&config)
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("config json: {e}"))))?;
            let ctx_src = format!("__cbCtx = __buildCtx({id_json}, {config_json}); __cbCtx;");
            let ctx_build_result: Result<Object, rquickjs::Error> = ctx.eval(ctx_src.as_str());
            let _ctx_obj: Object = ctx_build_result
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("ctx build: {e}"))))?;

            // activate(__cbCtx)
            let activate: Function = ctx.eval("__cbPlugin.activate").map_err(|e| {
                (
                    "INTERNAL_ERROR".into(),
                    Some(format!("activate lookup: {e}")),
                )
            })?;
            let cb_ctx: Value = ctx
                .eval("__cbCtx")
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("ctx ref: {e}"))))?;
            let act_result: Value = activate
                .call((cb_ctx,))
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(format!("activate call: {e}"))))?;
            if act_result.is_promise() {
                finish_promise(&ctx, act_result).map_err(|e| {
                    (
                        "INTERNAL_ERROR".into(),
                        Some(format!("activate await: {e}")),
                    )
                })?;
            }
            Ok(())
        });
        if let Err((_, msg)) = init_result {
            let detail = msg.unwrap_or_else(|| "unknown".into());
            // 若为 JS 异常，尝试取具体消息
            let extra = eng.ctx.with(|ctx| {
                if ctx.has_exception() {
                    let caught = ctx.catch();
                    ctx.json_stringify(caught)
                        .ok()
                        .flatten()
                        .and_then(|s| s.to_string().ok())
                        .unwrap_or_else(|| "<no exception value>".into())
                } else {
                    String::new()
                }
            });
            return Err((
                "INTERNAL_ERROR".into(),
                Some(format!("{detail} | exception: {extra}")),
            ));
        }
    }

    worker.engine = Some(engine);
    Ok(Json::Null)
}

fn dispose(worker: &mut Worker) -> Result<Json, Err2> {
    if worker.disposed {
        return Ok(Json::Null);
    }
    if let Some(engine) = &worker.engine {
        let engine_clone = engine.clone();
        let eng = engine_clone.borrow();
        eng.ctx.with(|ctx| {
            if let Ok(deact) = ctx.eval::<Function, _>("__cbPlugin.deactivate") {
                let result: Value = deact.call(()).map_err(js_err)?;
                if result.is_promise() {
                    finish_promise(&ctx, result).map_err(js_err)?;
                }
            }
            Ok(Json::Null)
        })?;
    }
    worker.disposed = true;
    Ok(Json::Null)
}

fn host_event(worker: &mut Worker, params: &Json) -> Result<Json, Err2> {
    if worker.engine.is_none() {
        return Err(("INVALID_MESSAGE".into(), Some("not initialized".into())));
    }
    let event = params
        .get("event")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let data = params.get("data").cloned().unwrap_or(Json::Null);
    let engine = worker.engine.clone().expect("checked above");
    let eng = engine.borrow();
    eng.ctx.with(|ctx| {
        let dispatch: Function = ctx.eval("__cbCtx.api.dispatchHostEvent").map_err(js_err)?;
        let event_json = serde_json::to_string(&event)
            .map_err(|e| ("INTERNAL_ERROR".into(), Some(e.to_string())))?;
        let data_json = serde_json::to_string(&data)
            .map_err(|e| ("INTERNAL_ERROR".into(), Some(e.to_string())))?;
        let ev: Value = ctx.json_parse(event_json.as_str()).map_err(js_err)?;
        let payload: Value = ctx.json_parse(data_json.as_str()).map_err(js_err)?;
        let _: Value = dispatch.call((ev, payload)).map_err(js_err)?;
        Ok(Json::Null)
    })
}

fn plugin_message(worker: &mut Worker, params: &Json) -> Result<Json, Err2> {
    if worker.engine.is_none() {
        return Err(("INVALID_MESSAGE".into(), Some("not initialized".into())));
    }
    let message = params.get("message").cloned().unwrap_or(Json::Null);
    let engine = worker.engine.clone().expect("checked above");
    let eng = engine.borrow();
    eng.ctx.with(|ctx| {
        let onmsg: Function = ctx.eval("__cbPlugin.onMessage").map_err(js_err)?;
        let payload_json = serde_json::to_string(&message)
            .map_err(|e| ("INTERNAL_ERROR".into(), Some(e.to_string())))?;
        let payload: Value = ctx.json_parse(payload_json.as_str()).map_err(js_err)?;
        let result: Value = onmsg.call((payload,)).map_err(js_err)?;
        let resolved = if result.is_promise() {
            finish_promise(&ctx, result).map_err(js_err)?
        } else {
            result
        };
        let out = ctx.json_stringify(resolved).map_err(js_err)?;
        let text = match out {
            Some(s) => s
                .to_string()
                .map_err(|e| ("INTERNAL_ERROR".into(), Some(e.to_string())))?,
            None => "null".into(),
        };
        serde_json::from_str(&text).map_err(|e| {
            (
                "INTERNAL_ERROR".into(),
                Some(format!("bad result json: {e}")),
            )
        })
    })
}

// ---------------------------------------------------------------------------
// polyfills & helpers

fn install_polyfills(ctx: &rquickjs::Ctx) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    let console = Object::new(ctx.clone())?;
    for name in ["info", "warn", "error", "debug"] {
        let name = name.to_string();
        let name2 = name.clone();
        let f = Function::new(ctx.clone(), move |msg: String| {
            eprintln!("[console.{name2}] {msg}");
        })?;
        console.set(name, f)?;
    }
    globals.set("console", console)?;
    let random_u32 = Function::new(ctx.clone(), || -> Result<u32, rquickjs::Error> {
        secure_random_u32()
            .map_err(|error| rquickjs::Error::new_from_js_message("crypto", "Uint32", error))
    })?;
    globals.set("__cbRandomUint32", random_u32)?;
    ctx.eval::<(), _>(
        r#"
        globalThis.crypto = {
          getRandomValues: function (arr) {
            if (!arr || typeof arr.length !== 'number') {
              throw new TypeError('getRandomValues expects a typed array');
            }
            for (var i = 0; i < arr.length; i++) {
              arr[i] = __cbRandomUint32();
            }
            return arr;
          }
        };
        "#,
    )?;
    Ok(())
}

/// Rust 全局 `__cjsResolve` / `__cjsLoad`，供 JS require 使用。
/// __cjsLoad 重新做根校验（C1：插件 JS 不能经它读任意文件）。
fn install_cjs_host_functions(
    ctx: &rquickjs::Ctx,
    plugin_root: &std::path::Path,
) -> rquickjs::Result<()> {
    let root = plugin_root.to_path_buf();
    let root_norm = loader::normalize_for_root(&root);
    let resolve_fn = Function::new(
        ctx.clone(),
        move |from_dir: String, specifier: String| -> Option<String> {
            let from = PathBuf::from(from_dir.replace('/', "\\"));
            let got = loader::resolve_specifier(&root, &from, &specifier)?;
            Some(got.to_string_lossy().replace('\\', "/"))
        },
    )?;
    ctx.globals().set("__cjsResolve", resolve_fn)?;

    let load_fn = Function::new(ctx.clone(), move |abs_path: String| -> Option<String> {
        let p = PathBuf::from(abs_path.replace('/', "\\"));
        // C1: 根校验——仅允许 plugin_root 内的文件
        let p_norm = loader::normalize_for_root(&p);
        if !p_norm.starts_with(&root_norm) {
            eprintln!("[host] __cjsLoad blocked path outside plugin dir: {abs_path}");
            return None;
        }
        std::fs::read_to_string(&p).ok()
    })?;
    ctx.globals().set("__cjsLoad", load_fn)?;
    Ok(())
}

fn finish_promise<'js>(
    ctx: &rquickjs::Ctx<'js>,
    promise: Value<'js>,
) -> Result<Value<'js>, rquickjs::Error> {
    let promise: rquickjs::Promise = rquickjs::Promise::from_value(promise)?;
    match promise.finish::<Value>() {
        Ok(v) => Ok(v),
        Err(rquickjs::Error::Exception) => Ok(ctx.catch()),
        Err(e) => Err(e),
    }
}

fn js_err(e: rquickjs::Error) -> Err2 {
    ("INTERNAL_ERROR".into(), Some(format!("js: {e}")))
}

#[cfg(test)]
mod random_tests {
    use super::secure_random_u32;

    #[test]
    fn secure_random_source_produces_full_width_words() {
        let samples = (0..16)
            .map(|_| secure_random_u32().expect("OS random source must be available"))
            .collect::<Vec<_>>();
        assert!(samples.iter().any(|sample| *sample > 255));
    }
}
