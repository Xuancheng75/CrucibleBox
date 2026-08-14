use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use tauri::http::{header::CONTENT_TYPE, Request, Response, StatusCode};
use tauri::Manager;

fn report_path() -> PathBuf {
    // 绝对路径日志，避免 cwd 依赖
    let base = std::env::var("TEMP").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("poc-tauri-report.log")
}

/// 把 [poc] 标记写入绝对路径日志（GUI 子系统下 stdout 不可靠）
fn report(line: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(report_path())
    {
        let _ = writeln!(f, "{line}");
    }
}

/// 最小插件资源服务器：openbox-plugin://plugin/<path>
/// 复刻 CrucibleBox 插件 iframe 场景：宿主页 index.html 内嵌 sandboxed iframe，
/// iframe src 指向本协议；插件目录含 index.html + 一个 js 子资源（#11505 关注点）。
const PLUGIN_INDEX_HTML: &str = r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>plugin-frame</title>
  </head>
  <body>
    <h1>PLUGIN IFRAME (custom protocol)</h1>
    <p id="sub">plugin-body</p>
    <script src="plugin.js"></script>
    <script>
      var statusEl = document.getElementById('sub');
      var origin = location.origin;
      statusEl.textContent = 'plugin-loaded origin=' + origin;
      window.addEventListener('message', function (e) {
        statusEl.textContent = 'message-received origin=' + origin + ' data=' + JSON.stringify(e.data);
        e.source.postMessage({ kind: 'plugin-reply', origin: origin }, '*');
      });
      // 上报 iframe 内可观察状态
      window.parent.postMessage({ kind: 'plugin-status', origin: origin, text: statusEl.textContent }, '*');
    </script>
  </body>
</html>
"#;

const PLUGIN_JS: &str = r#"
  (function () {
    var el = document.getElementById('sub');
    if (el) el.textContent = 'subresource-ok';
    window.parent.postMessage({ kind: 'plugin-subresource', origin: location.origin, text: el.textContent }, '*');
  })();
"#;

fn make_response(body: &'static str, content_type: &'static str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header("Cross-Origin-Resource-Policy", "cross-origin")
        // PoC: 允许宿主页跨源访问（Windows origin 为 http://openbox-plugin.localhost）
        .header("Access-Control-Allow-Origin", "*")
        .body(body.as_bytes().to_vec())
        .expect("valid response")
}

fn main() {
    report(&format!("[boot] main() start, TEMP={}", report_path().display()));
    tauri::Builder::default()
        .setup(|app| {
            report("[boot] setup() entered");
            let window = app.get_webview_window("main").expect("main window");
            report("[boot] main window acquired");
            let _ = window.eval(
                r#"
                console.log('[poc] window ready');
                window.__POC_REPORT__ = function (line) {
                  try { window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke('report_status', { message: line }) } catch (e) {}
                  console.log('[poc] ' + line)
                };
                "#,
            );
            report("[boot] setup() done");
            Ok(())
        })
        // 前端宿主页把 iframe 检测结果上报到这里，一并写日志
        .invoke_handler(tauri::generate_handler![report_status])
        // 自定义协议：Windows 上 origin 形如 http://openbox-plugin.localhost/
        .register_uri_scheme_protocol("openbox-plugin", |_ctx, request: Request<Vec<u8>>| {
            let uri = request.uri();
            let path = uri.path().trim_start_matches('/');
            report(&format!("[protocol-request] {uri} (path={path})"));
            match path {
                "plugin/index.html" | "index.html" | "" => {
                    make_response(PLUGIN_INDEX_HTML, "text/html")
                }
                "plugin/plugin.js" | "plugin.js" => {
                    make_response(PLUGIN_JS, "application/javascript")
                }
                other => {
                    report(&format!("[protocol-404] {other}"));
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Vec::new())
                        .expect("valid 404")
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 宿主页调用：上报 iframe 加载状态
#[tauri::command]
fn report_status(message: String) {
    report(&format!("[host-status] {message}"));
}
