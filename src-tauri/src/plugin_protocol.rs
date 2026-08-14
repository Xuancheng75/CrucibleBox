// 插件 renderer 协议 handler（1.8.3，对等 plugin-system/PluginRendererProtocol.ts）
// Tauri 自定义协议：http://cruciblebox-plugin.localhost/<token>/index.html（path 型）
// 资源路由：
//   /<token>/index.html   → 校验 session（一次性消费 issued→active）→ 生成 index
//   /<token>/runtime.js   → 宿主打包的 frame runtime（out/plugin-frame/runtime.js）
//   /<token>/renderer.js  → 插件 dist/renderer.js（防穿越 + 白名单 MIME）

use crate::plugin_session::{RendererSession, RendererSessionRegistry, SessionAccess};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const SAFE_MIME: &[(&str, &str)] = &[
    (".js", "text/javascript; charset=utf-8"),
    (".css", "text/css; charset=utf-8"),
    (".html", "text/html; charset=utf-8"),
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".svg", "image/svg+xml"),
    (".json", "application/json"),
    (".woff", "font/woff"),
    (".woff2", "font/woff2"),
];

fn mime_for(path: &str) -> Option<&'static str> {
    let ext = path
        .rsplit('.')
        .next()
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    SAFE_MIME.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

fn escape_html_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn generated_index(session: &RendererSession) -> String {
    let renderer_script = if session.renderer_api_version == 2 {
        "\n    <script src=\"/renderer.js\"></script>"
    } else {
        ""
    };
    format!(
        "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"utf-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n    <title></title>\n  </head>\n  <body>\n    <div id=\"root\" data-session-token=\"{}\" data-api-version=\"{}\" data-renderer-url=\"/renderer.js\"></div>\n    <script src=\"/runtime.js\"></script>{}\n  </body>\n</html>",
        escape_html_attr(&session.handshake_token),
        session.renderer_api_version,
        renderer_script
    )
}

/// 从 URL path 提取 session token（path 型 /<token>/<resource>）。
fn token_from_path(pathname: &str) -> Option<(String, String)> {
    let path = pathname.split('?').next().unwrap_or(pathname);
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    let token = segments.next()?;
    let resource = segments.next().unwrap_or("index.html").to_string();
    if token.len() != 64 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((token.to_string(), resource))
}

/// 资源解析：仅允许 session.pluginDirectory 内（对等 resolveSafeAsset + isContained）。
fn resolve_safe_asset(session: &RendererSession, resource: &str) -> Option<PathBuf> {
    if !resource.starts_with('/')
        || resource.contains('\0')
        || resource.contains('\\')
        || resource.split('/').any(|s| s == "." || s == "..")
    {
        return None;
    }
    let relative = &resource[1..];
    if relative.is_empty() || relative.split('/').any(|s| s.is_empty()) {
        return None;
    }
    mime_for(relative)?;
    let root = PathBuf::from(&session.plugin_directory);
    let candidate = root.join(relative);
    // 规范化后必须仍在 pluginDirectory 内
    let norm_root = normalize_win(&root);
    let norm_candidate = normalize_win(&candidate);
    if !norm_candidate.starts_with(&norm_root) {
        return None;
    }
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn normalize_win(path: &std::path::Path) -> PathBuf {
    use std::path::Component;
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

pub struct ProtocolContext {
    pub registry: Arc<Mutex<RendererSessionRegistry>>,
    pub owner_label: String,
}

/// Tauri URI scheme handler 主体。uri 形如 http://cruciblebox-plugin.localhost/<token>/<resource>
pub fn handle_protocol(ctx: &ProtocolContext, uri: String) -> tauri::http::Response<Vec<u8>> {
    let response = handle_inner(ctx, &uri);
    match response {
        Ok((status, content_type, body)) => tauri::http::Response::builder()
            .status(status)
            .header("Content-Type", content_type)
            .header("Cross-Origin-Resource-Policy", "cross-origin")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)
            .unwrap_or_else(|_| {
                tauri::http::Response::builder()
                    .status(500)
                    .body(Vec::new())
                    .unwrap()
            }),
        Err((status, message)) => tauri::http::Response::builder()
            .status(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(message.into_bytes())
            .unwrap_or_else(|_| {
                tauri::http::Response::builder()
                    .status(500)
                    .body(Vec::new())
                    .unwrap()
            }),
    }
}

type HandlerResult = Result<(u16, &'static str, Vec<u8>), (u16, String)>;

fn handle_inner(ctx: &ProtocolContext, uri: &str) -> HandlerResult {
    // 解析 URL
    let pathname = uri
        .split("://")
        .nth(1)
        .map(|rest| {
            // 去掉 host（<scheme>.localhost），剩余 /<token>/<resource>
            rest.split_once('/')
                .map(|(_, p)| format!("/{p}"))
                .unwrap_or("/".into())
        })
        .unwrap_or_else(|| uri.to_string());

    let (token, resource) =
        token_from_path(&pathname).ok_or((400, "invalid session token".to_string()))?;

    // 路由
    match resource.as_str() {
        "index.html" => {
            let mut reg = ctx.registry.lock().unwrap();
            let access = reg.consume_index(&token, &ctx.owner_label);
            match access.ok {
                true => {
                    let session = access.session.ok_or((500, "session missing".into()))?;
                    Ok((
                        200,
                        "text/html; charset=utf-8",
                        generated_index(&session).into_bytes(),
                    ))
                }
                false => {
                    let reason = access
                        .reason
                        .map(|r| format!("{r:?}"))
                        .unwrap_or_else(|| "denied".into());
                    Err((403, reason))
                }
            }
        }
        "runtime.js" => {
            let reg = ctx.registry.lock().unwrap();
            let access = reg.get(&token, &ctx.owner_label);
            let session = access.session.ok_or((403, "session denied".into()))?;
            let body = std::fs::read(&session.runtime_path)
                .map_err(|e| (404, format!("runtime missing: {e}")))?;
            Ok((200, "text/javascript; charset=utf-8", body))
        }
        _ => {
            // renderer.js 或其他插件资源
            let reg = ctx.registry.lock().unwrap();
            let access = reg.get(&token, &ctx.owner_label);
            let session = access.session.ok_or((403, "session denied".into()))?;
            let path = resolve_safe_asset(&session, &format!("/{resource}"))
                .ok_or((404, "asset not found".into()))?;
            let body =
                std::fs::read(&path).map_err(|e| (404, format!("asset read failed: {e}")))?;
            let mime = mime_for(&resource).unwrap_or("application/octet-stream");
            Ok((200, mime, body))
        }
    }
}

/// 会话 JSON（供 create-renderer-session 命令返回）
pub fn session_dto(session: &RendererSession) -> serde_json::Value {
    json!({
        "token": session.token,
        "handshakeToken": session.handshake_token,
        "origin": session.origin,
        "indexUrl": session.index_url,
        "rendererApiVersion": session.renderer_api_version,
        "expiresAt": session.expires_at_ms,
    })
}

/// 便捷访问器（供 1.8.4 宿主集成使用）
#[allow(dead_code)]
pub fn access_session(ctx: &ProtocolContext, token: &str) -> SessionAccess {
    ctx.registry.lock().unwrap().get(token, &ctx.owner_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> (ProtocolContext, String) {
        let mut reg = RendererSessionRegistry::new(crate::plugin_session::DEFAULT_TTL);
        let session = reg
            .create(crate::plugin_session::CreateSessionInput {
                plugin_id: "diary".into(),
                plugin_name: "diary".into(),
                plugin_directory: "C:/plugins/diary".into(),
                renderer_entry: "dist/renderer.js".into(),
                runtime_path: "C:/app/out/plugin-frame/runtime.js".into(),
                renderer_api_version: 2,
                permissions: vec![],
                owner_webview_label: "main".into(),
            })
            .unwrap();
        let token = session.token.clone();
        (
            ProtocolContext {
                registry: Arc::new(Mutex::new(reg)),
                owner_label: "main".into(),
            },
            token,
        )
    }

    #[test]
    fn protocol_serves_index_and_consumes_once() {
        let (ctx, token) = make_ctx();
        let uri = format!("http://cruciblebox-plugin.localhost/{token}/index.html");
        let resp = handle_inner(&ctx, &uri).unwrap();
        assert_eq!(resp.0, 200);
        let body = String::from_utf8(resp.2).unwrap();
        assert!(body.contains("data-session-token="));
        assert!(body.contains("runtime.js"));
        // 二次访问 index → 403 already-consumed
        let again = handle_inner(&ctx, &uri);
        assert!(again.is_err());
        assert_eq!(again.err().unwrap().0, 403);
    }

    #[test]
    fn protocol_rejects_unknown_token() {
        let (ctx, _) = make_ctx();
        let uri = format!(
            "http://cruciblebox-plugin.localhost/{}/index.html",
            "c".repeat(64)
        );
        let resp = handle_inner(&ctx, &uri);
        assert!(resp.is_err());
        assert_eq!(resp.err().unwrap().0, 403);
    }

    #[test]
    fn token_from_path_parses() {
        let token = "a".repeat(64);
        let (t, r) = token_from_path(&format!("/{token}/index.html")).unwrap();
        assert_eq!(t, token);
        assert_eq!(r, "index.html");
        let (t2, r2) = token_from_path(&format!("/{token}/renderer.js")).unwrap();
        assert_eq!(t2, token);
        assert_eq!(r2, "renderer.js");
        assert!(token_from_path("/bad/index.html").is_none());
    }

    #[test]
    fn resolve_safe_asset_rejects_escape() {
        let session = RendererSession {
            token: "a".repeat(64),
            handshake_token: "b".repeat(64),
            origin: "http://cruciblebox-plugin.localhost".into(),
            index_url: "http://cruciblebox-plugin.localhost/index.html".into(),
            plugin_id: "x".into(),
            plugin_name: "x".into(),
            plugin_directory: "C:/plugins/x".into(),
            renderer_entry: "dist/renderer.js".into(),
            renderer_path: "C:/plugins/x/dist/renderer.js".into(),
            runtime_path: "C:/app/runtime.js".into(),
            renderer_api_version: 2,
            permissions: vec![],
            owner_webview_label: "main".into(),
            created_at_ms: 0,
            expires_at_ms: u64::MAX,
            state: "issued",
        };
        assert!(resolve_safe_asset(&session, "/../../evil.js").is_none());
        assert!(resolve_safe_asset(&session, "/dist/../renderer.js").is_none());
        assert!(resolve_safe_asset(&session, "/dist/renderer.js").is_none()); // 文件不存在
                                                                              // mime 白名单外
        assert!(resolve_safe_asset(&session, "/dist/evil.exe").is_none());
    }
}
