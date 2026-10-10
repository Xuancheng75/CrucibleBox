//! Next-only backend process. Legacy ctx/lifecycle dispatch is historical test code.
#[cfg(test)]
include!("legacy_entry.rs");
#[cfg(not(test))]
mod loader;
#[cfg(not(test))]
mod next_worker;
#[cfg(not(test))]
use rquickjs::{Function, Object};
#[cfg(all(not(test), unix))]
use std::io::Read;
#[cfg(not(test))]
use std::path::PathBuf;
#[cfg(not(test))]
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

#[cfg(not(test))]
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
#[cfg(not(test))]
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

#[cfg(not(test))]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4
        || args[2] != "3"
        || !(32..=128).contains(&args[3].len())
        || !args[3].bytes().all(|b| b.is_ascii_alphanumeric())
    {
        eprintln!("LEGACY_RUNTIME_RETIRED: expected Next wire 3 worker arguments");
        std::process::exit(1);
    }
    let directory = PathBuf::from(&args[0]);
    let entry = directory.join(&args[1]);
    if let Err(error) = next_worker::run(directory, entry, args[3].clone()) {
        eprintln!("[next-worker] {error}");
        std::process::exit(1);
    }
}
