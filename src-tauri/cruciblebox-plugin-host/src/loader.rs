// CommonJS loader（1.8.2）
// 对等 PluginProcessEntry.resolvePluginMain 语义 + tsc 派的目录内相对 require。
// 设计：Rust 侧提供两个 JS 全局——
//   __cjsResolve(fromDir, specifier) -> 绝对路径（防逃逸：仅允许 pluginDir 内）| null
//   __cjsLoad(absPath) -> 文件内容 | null
// JS 侧 require() 负责模块缓存与执行（new Function 注入 module/exports/require）。

use std::path::{Component, Path, PathBuf};

/// 解析相对/绝对模块路径。安全规则（对等 resolvePluginMain 防逃逸）：
/// - 相对 specifier 必须以 ./ 或 ../ 开头（拒绝裸模块名）
/// - 绝对路径（盘符或 / 开头）允许，但必须落在 plugin_root 内
/// - 解析后的真实路径必须在 plugin_root 内（对 .. 逃逸返回 None）
/// - 尝试 specifier 原样、+ .js、+ /index.js
pub fn resolve_specifier(plugin_root: &Path, from_dir: &Path, specifier: &str) -> Option<PathBuf> {
    let candidate = if specifier.starts_with("./") || specifier.starts_with("../") {
        from_dir.join(specifier)
    } else if specifier.contains('/') || specifier.contains('\\') {
        // 绝对路径（盘符/根）或含目录分隔符的引用：直接按路径处理，交给根校验
        PathBuf::from(specifier)
    } else {
        return None; // 裸模块名拒绝
    };

    let normalized = normalize(&candidate);
    let root_norm = normalize(plugin_root);
    if !normalized.starts_with(&root_norm) {
        return None; // 逃逸插件目录
    }

    let with_js = if normalized.extension().is_some() {
        vec![normalized.clone()]
    } else {
        vec![
            normalized.clone(),
            normalized.with_extension("js"),
            normalized.join("index.js"),
        ]
    };
    with_js.into_iter().find(|path| path.is_file())
}

/// 规范化路径（消除 ./ 与 .. 组件），用于安全前缀比对。公开给 __cjsLoad 复用。
pub fn normalize_for_root(path: &Path) -> PathBuf {
    normalize(path)
}

/// 规范化路径（消除 ./ 与 .. 组件），用于安全前缀比对。
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 注入到 JS 的 require 引导代码（模块缓存 + new Function 执行 + 相对目录追踪）。
pub const LOADER_JS: &str = r#"
var __cjsModules = {};
var __cjsCurrentDir = null;
globalThis.require = function (specifier) {
  if (__cjsCurrentDir === null) {
    throw new Error('require called before module load context');
  }
  var abs = globalThis.__cjsResolve(__cjsCurrentDir, specifier);
  if (!abs) {
    throw new Error('module not found or outside plugin dir: ' + specifier);
  }
  if (__cjsModules[abs]) {
    return __cjsModules[abs].exports;
  }
  var src = globalThis.__cjsLoad(abs);
  if (src === null) {
    throw new Error('cannot read module: ' + abs);
  }
  var module = { exports: {} };
  __cjsModules[abs] = module;
  var prevDir = __cjsCurrentDir;
  __cjsCurrentDir = abs.substring(0, abs.lastIndexOf('/'));
  var fn;
  try {
    fn = new Function('module', 'exports', 'require', src);
  } catch (e) {
    __cjsCurrentDir = prevDir;
    throw e;
  }
  try {
    fn(module, module.exports, globalThis.require);
  } finally {
    __cjsCurrentDir = prevDir;
  }
  return module.exports;
};
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_plugin(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cruciblebox-cjs-test-{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("dist").join("sub")).unwrap();
        fs::write(dir.join("dist").join("main.js"), "exports.default = {}").unwrap();
        fs::write(dir.join("dist").join("domain.js"), "exports.x = 1").unwrap();
        fs::write(
            dir.join("dist").join("sub").join("index.js"),
            "exports.y = 2",
        )
        .unwrap();
        dir
    }

    #[test]
    fn resolves_relative_with_js_extension() {
        let root = temp_plugin("rel-js");
        let from = root.join("dist");
        let got = resolve_specifier(&root, &from, "./domain.js").unwrap();
        assert!(got.ends_with("domain.js"));
        // 无扩展名时补 .js
        let got2 = resolve_specifier(&root, &from, "./domain").unwrap();
        assert!(got2.ends_with("domain.js"));
    }

    #[test]
    fn resolves_directory_index() {
        let root = temp_plugin("index");
        let from = root.join("dist");
        let got = resolve_specifier(&root, &from, "./sub").unwrap();
        assert!(got.ends_with("sub\\index.js") || got.ends_with("sub/index.js"));
    }

    #[test]
    fn rejects_escape_attempts() {
        let root = temp_plugin("escape");
        let from = root.join("dist");
        assert!(resolve_specifier(&root, &from, "../../etc/passwd").is_none());
        assert!(resolve_specifier(&root, &from, "bare-module").is_none());
        assert!(resolve_specifier(&root, &from, "../..").is_none());
        // 绝对路径逃逸插件根 → 拒绝
        let outside = std::env::temp_dir().join("cruciblebox-outside-x");
        let _ = fs::create_dir_all(&outside);
        let abs_escape = outside.join("evil.js");
        fs::write(&abs_escape, "x").unwrap();
        let abs_str = abs_escape.to_string_lossy().to_string();
        assert!(resolve_specifier(&root, &from, &abs_str).is_none());
    }

    #[test]
    fn allows_absolute_path_inside_root() {
        let root = temp_plugin("abs");
        let from = root.join("dist");
        let abs_main = root.join("dist").join("main.js");
        let abs_str = abs_main.to_string_lossy().to_string();
        let got = resolve_specifier(&root, &from, &abs_str).unwrap();
        assert!(got.ends_with("main.js"));
    }

    #[test]
    fn rejects_missing_file() {
        let root = temp_plugin("missing");
        let from = root.join("dist");
        assert!(resolve_specifier(&root, &from, "./nope.js").is_none());
    }
}
