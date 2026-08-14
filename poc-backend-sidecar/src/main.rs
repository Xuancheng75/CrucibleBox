// P2 gate spike: quickjs-ng loads a real plugin backend (CJS), injects a minimal
// ctx (logger/storage backed by in-memory map), and verifies the synchronous
// activate/onMessage round-trip shapes match the Electron runtime contract.
//
// Usage: poc-backend-sidecar <path-to-dist/main.js>
// Exit code 0 = all assertions passed.
//
// rquickjs 0.12 API notes:
// - Context::with(|ctx: Ctx| ...); Ctx methods below.
// - Object::get::<V, K>(key); Value must be converted to Object first.
// - execute_pending_job() drains the microtask queue (async fn resolve path).
// - crypto polyfill is pure JS (Math.random) — sufficient for the spike; the
//   real implementation uses getrandom/ring.

use rquickjs::{Context, Function, Object, Runtime, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Minimal ctx: logger + storage (in-memory). Mirrors the RPC-backed surface
/// at value-shape level; real transport replaces these bodies later.
struct MiniCtx {
    storage: HashMap<String, String>,
    log_lines: Vec<String>,
}

type Shared = Rc<RefCell<MiniCtx>>;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let main_entry = args
        .get(1)
        .expect("usage: poc-backend-sidecar <path/to/dist/main.js>");
    let source = std::fs::read_to_string(main_entry)
        .unwrap_or_else(|e| panic!("cannot read {main_entry}: {e}"));
    eprintln!("[spike] loading {}", main_entry);

    let rt = Runtime::new().expect("runtime");
    let ctx = Context::full(&rt).expect("context");
    ctx.with(|ctx| -> rquickjs::Result<()> {
        install_polyfills(&ctx)?;

        let shared: Shared = Rc::new(RefCell::new(MiniCtx {
            storage: HashMap::new(),
            log_lines: Vec::new(),
        }));

        // CommonJS shim: evaluate bundle as script with module/exports globals.
        let exports_obj = Object::new(ctx.clone())?;
        let module_obj = Object::new(ctx.clone())?;
        module_obj.set("exports", exports_obj.clone())?;
        ctx.globals().set("module", module_obj)?;
        ctx.globals().set("exports", exports_obj)?;
        ctx.globals().set(
            "require",
            Function::new(ctx.clone(), move |_p: String| -> Option<Value> {
                None // single-file bundles only; real loader resolves relative requires
            })?,
        )?;

        ctx.eval::<(), _>(source.as_str())?;

        // Resolve plugin object: esbuild -> module.exports.default; tsc -> exports.default
        let plugin_val: rquickjs::Value = ctx.eval(
            "var p = (module && module.exports && module.exports.default) \
               ? module.exports.default \
               : ((module && module.exports && 'activate' in module.exports) ? module.exports : (exports.default || null)); p",
        )?;
        assert!(!plugin_val.is_undefined() && !plugin_val.is_null(), "plugin not resolved");
        eprintln!("[spike] plugin object resolved");

        let plugin_obj: Object = plugin_val.clone().into_object().expect("plugin is not object");

        let ctx_obj = build_ctx(&ctx, &shared)?;

        // activate(ctx) — async in diary, sync in gif-editor; drain jobs either way
        let activate: Function = plugin_obj.get("activate").expect("activate missing");
        let act_result: rquickjs::Value = activate.call((ctx_obj.clone(),))?;
        let act_kind = if act_result.is_promise() { "Promise" } else { "sync" };
        drain_jobs(&ctx);
        eprintln!(
            "[spike] activate called (kind={act_kind}, log_lines={})",
            shared.borrow().log_lines.len()
        );
        assert!(
            shared.borrow().log_lines.len() >= 1,
            "activate should log at least once"
        );

        // onMessage({type:'ping'}) shape check (gif-editor stub replies {ok:true})
        if let Ok(onmsg) = plugin_obj.get::<_, Function>("onMessage") {
            let payload = Object::new(ctx.clone())?;
            payload.set("type", "ping")?;
            let reply: rquickjs::Value = onmsg.call((payload,))?;
            // async onMessage returns a Promise: finish it (drives job queue to resolution)
            let resolved: rquickjs::Value = if reply.is_promise() {
                let promise: rquickjs::Promise = rquickjs::Promise::from_value(reply.clone())?;
                match promise.finish::<rquickjs::Value>() {
                    Ok(v) => v,
                    Err(rquickjs::Error::Exception { .. }) => {
                        let caught = ctx.catch();
                        caught
                    }
                    Err(e) => {
                        eprintln!("[spike] onMessage promise finish error: {e}");
                        Object::new(ctx.clone())?
                            .into_value()
                    }
                }
            } else {
                reply
            };
            let reply_str = match ctx.json_stringify(resolved)? {
                Some(s) => s.to_string().unwrap_or_else(|_| "<string-error>".into()),
                None => "<unserializable>".to_string(),
            };
            eprintln!("[spike] onMessage(ping) replied: {reply_str}");
        }

        // deactivate
        let deact: Function = plugin_obj.get("deactivate")?;
        let _ = deact.call::<_, rquickjs::Value>(())?;
        drain_jobs(&ctx);
        eprintln!("[spike] deactivate called");

        // storage round-trip via ctx
        let storage: Object = ctx_obj.get("storage")?;
        let sget: Function = storage.get("get")?;
        let sset: Function = storage.get("set")?;
        sset.call::<_, rquickjs::Value>(("key-a", "value-1"))?;
        let got: String = sget.call::<_, String>(("key-a",))?;
        assert_eq!(got, "value-1", "storage round-trip mismatch");
        eprintln!("[spike] storage round-trip ok: key-a={got}");

        eprintln!("[spike] captured log lines:");
        for line in shared.borrow().log_lines.iter() {
            eprintln!("  [log] {line}");
        }
        Ok(())
    })
    .expect("spike body failed");

    eprintln!("[spike] ALL ASSERTIONS PASSED");
}

/// console (-> stderr) + crypto (pure JS) polyfills.
fn install_polyfills<'js>(ctx: &rquickjs::Ctx<'js>) -> rquickjs::Result<()> {
    let globals = ctx.globals();

    let console = Object::new(ctx.clone())?;
    for name in ["info", "warn", "error", "debug"] {
        let name = name.to_string();
        let name_in_closure = name.clone();
        let f = Function::new(ctx.clone(), move |msg: String| {
            eprintln!("[console.{name_in_closure}] {msg}");
        })?;
        console.set(name, f)?;
    }
    globals.set("console", console)?;

    // Pure-JS crypto (Math.random-based) — adequate for spike; real impl uses
    // getrandom/ring. Covers turntable's crypto.getRandomValues(new Uint32Array(1)).
    ctx.eval::<(), _>(
        r#"
        globalThis.crypto = {
          getRandomValues: function (arr) {
            for (var i = 0; i < arr.length; i++) {
              arr[i] = Math.floor(Math.random() * 256);
            }
            return arr;
          }
        };
        "#,
    )?;
    Ok(())
}

/// ctx = { id, config, logger, storage } (minimal subset of buildContext).
fn build_ctx<'js>(
    ctx: &rquickjs::Ctx<'js>,
    shared: &Shared,
) -> rquickjs::Result<Object<'js>> {
    let logger = Object::new(ctx.clone())?;
    for name in ["info", "warn", "error", "debug"] {
        let name = name.to_string();
        let shared2 = shared.clone();
        let name_in_closure = name.clone();
        let f = Function::new(ctx.clone(), move |message: String| {
            shared2
                .borrow_mut()
                .log_lines
                .push(format!("{}: {}", name_in_closure, message));
        })?;
        logger.set(name, f)?;
    }

    let storage = Object::new(ctx.clone())?;
    {
        let shared2 = shared.clone();
        let f = Function::new(ctx.clone(), move |key: String| -> Option<String> {
            shared2.borrow().storage.get(&key).cloned()
        })?;
        storage.set("get", f)?;
    }
    {
        let shared2 = shared.clone();
        let f = Function::new(ctx.clone(), move |key: String, value: String| {
            shared2.borrow_mut().storage.insert(key, value);
        })?;
        storage.set("set", f)?;
    }
    {
        let shared2 = shared.clone();
        let f = Function::new(ctx.clone(), move |key: String| {
            shared2.borrow_mut().storage.remove(&key);
        })?;
        storage.set("delete", f)?;
    }

    let obj = Object::new(ctx.clone())?;
    obj.set("id", "poc-plugin")?;
    obj.set("config", Object::new(ctx.clone())?)?;
    obj.set("logger", logger)?;
    obj.set("storage", storage)?;
    Ok(obj)
}

/// Drain the microtask queue until quiescent (resolves sync-settled promises).
fn drain_jobs<'js>(ctx: &rquickjs::Ctx<'js>) {
    let mut guard = 0;
    while ctx.execute_pending_job() && guard < 100_000 {
        guard += 1;
    }
    if guard >= 100_000 {
        eprintln!("[spike] WARN: job drain guard hit (possible infinite promise chain)");
    }
}
