// UniEnv trusted host service（1.9.9，阶段 A）
// 对等 Electron 线 plugin-system/trusted-services/unienv/（已冻结）。本实现运行在宿主
// Rust 进程：sidecar 内插件经 api.invokeTrustedService('unienv', ...) → __hostRequest
// "trusted.invoke" → envelope_host::host_dispatch → 本模块。
//
// 阶段 A 范围（只读核心）：
//   listTools / listVersions / listCombos / detect
//   activate / deactivate（状态管理）
//   install / uninstall / switchVersion / installCombo → 返回明确"后续阶段"错误，
//   不执行任何下载/写操作（避免半成品破坏环境）。
// 元数据（工具清单/版本/组合包/制品 URL+SHA）静态移植自 Electron 线，保持契约一致。

use serde_json::{json, Value};
use std::path::PathBuf;

#[allow(dead_code)]
const SUPPORTED_MIRRORS: [&str; 4] = ["direct", "huawei", "aliyun", "tuna"];

fn supported_versions() -> Value {
    json!({
        "python": ["3.8.10", "3.9.13", "3.10.11", "3.11.9", "3.12.5", "3.14.7"],
        "node": ["16.20.2", "18.20.4", "20.15.1", "22.5.1", "24.18.1"],
        "git": ["2.43.0", "2.44.0", "2.45.2", "2.46.0", "2.54.0"],
        "go": ["1.21.6", "1.22.4", "1.23.0", "1.26.5"],
        "java": ["17.0.11", "17.0.12", "17.0.20", "21.0.3", "21.0.5", "21.0.12", "22.0.1", "25.0.4"]
    })
}

fn tool_meta() -> Value {
    json!([
        { "id": "python", "displayName": "Python", "icon": "\u{1F40D}", "description": "Python 编程语言运行时" },
        { "id": "node", "displayName": "Node.js", "icon": "\u{1F4E6}", "description": "JavaScript 运行时" },
        { "id": "git", "displayName": "Git", "icon": "\u{1F527}", "description": "分布式版本控制" },
        { "id": "go", "displayName": "Go", "icon": "\u{1F4C0}", "description": "Go 编程语言工具链" },
        { "id": "java", "displayName": "Java (JDK)", "icon": "\u{2615}\u{FE0F}", "description": "Java 开发工具包 (Temurin)" }
    ])
}

fn builtin_combos() -> Value {
    json!([
        {
            "id": "python-fullstack", "name": "Python 全栈", "description": "Python 3.14 + Node 24 + Git",
            "items": [ { "toolId": "python", "version": "3.14.7" }, { "toolId": "node", "version": "24.18.1" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "java-dev", "name": "Java 开发", "description": "JDK 21 + Git",
            "items": [ { "toolId": "java", "version": "21.0.12" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "go-dev", "name": "Go 开发", "description": "Go 1.26 + Git",
            "items": [ { "toolId": "go", "version": "1.26.5" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "frontend-dev", "name": "前端开发", "description": "Node 24 + Git",
            "items": [ { "toolId": "node", "version": "24.18.1" }, { "toolId": "git", "version": "2.54.0" } ]
        },
        {
            "id": "fullstack-universal", "name": "全栈通用", "description": "Python 3.14 + Node 24 + Go 1.26 + Git",
            "items": [ { "toolId": "python", "version": "3.14.7" }, { "toolId": "node", "version": "24.18.1" }, { "toolId": "go", "version": "1.26.5" }, { "toolId": "git", "version": "2.54.0" } ]
        }
    ])
}

/// 判定 tool id 是否受支持
fn is_supported_tool(tool: &str) -> bool {
    supported_versions().get(tool).is_some()
}

/// 判定 (tool, version) 是否受支持
#[allow(dead_code)] // 阶段 B（install/switch）启用
fn is_supported_version(tool: &str, version: &str) -> bool {
    supported_versions()
        .get(tool)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|x| x.as_str() == Some(version)))
        .unwrap_or(false)
}

/// 版本目录：<installRoot>\<tool>\<version>
fn version_dir(install_root: &str, tool: &str, version: &str) -> PathBuf {
    PathBuf::from(install_root).join(tool).join(version)
}

/// 检测某工具是否已安装：<installRoot>\<tool>\current junction 或版本目录存在
fn detect_tool(install_root: &str, tool: &str) -> Value {
    let tool_dir = PathBuf::from(install_root).join(tool);
    let current = tool_dir.join("current");
    let current_meta = std::fs::symlink_metadata(&current).ok();
    if let Some(meta) = current_meta {
        if meta.file_type().is_symlink() {
            // junction 目标即当前版本
            if let Ok(target) = std::fs::read_link(&current) {
                if let Some(name) = target.file_name() {
                    return json!({
                        "installed": true,
                        "version": name.to_string_lossy(),
                        "path": current.to_string_lossy(),
                    });
                }
            }
            return json!({ "installed": true, "path": current.to_string_lossy() });
        }
    }
    // 回退：存在已装版本目录
    let versions = supported_versions()
        .get(tool)
        .and_then(Value::as_array)
        .cloned();
    if let Some(list) = versions {
        for v in list {
            if let Some(ver) = v.as_str() {
                if version_dir(install_root, tool, ver).is_dir() {
                    return json!({
                        "installed": true,
                        "version": ver,
                        "path": version_dir(install_root, tool, ver).to_string_lossy(),
                    });
                }
            }
        }
    }
    json!({ "installed": false })
}

/// 处理 unienv 'message' 操作
fn handle_message(payload: &Value) -> Value {
    let request = match payload {
        Value::Object(_) => payload,
        _ => {
            return json!({ "error": "message payload must be an object", "code": "invalid-value" })
        }
    };
    let msg_type = request
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // 从 config.installRoot 读取（阶段 A：默认 C:\UniEnv；后续接插件 config）
    let install_root = request
        .get("installRoot")
        .and_then(Value::as_str)
        .unwrap_or("C:\\UniEnv")
        .to_string();

    match msg_type.as_str() {
        "listTools" => json!(tool_meta()),
        "listVersions" => {
            let tool = request.get("tool").and_then(Value::as_str).unwrap_or("");
            if !is_supported_tool(tool) {
                return json!({ "error": format!("unsupported tool: {tool}"), "code": "unknown-tool" });
            }
            json!(supported_versions()
                .get(tool)
                .cloned()
                .unwrap_or_else(|| json!([])))
        }
        "listCombos" => json!(builtin_combos()),
        "detect" => {
            let tool = request.get("tool").and_then(Value::as_str).unwrap_or("");
            if !is_supported_tool(tool) {
                return json!({ "error": format!("unsupported tool: {tool}"), "code": "unknown-tool" });
            }
            detect_tool(&install_root, tool)
        }
        "install" | "switchVersion" | "uninstall" | "installCombo" | "getTask" | "cancelTask" => {
            json!({
                "error": format!("UniEnv {} 在 1.9.9 阶段 A 尚未支持，将在后续版本提供", msg_type),
                "code": "not-supported-yet"
            })
        }
        _ => {
            json!({ "error": format!("unknown message type: {msg_type}"), "code": "unknown-type" })
        }
    }
}

/// 统一入口：envelope_host::host_dispatch 分发 "trusted.invoke" 时调用。
/// params: { service, operation, payload? }。仅接受 service == "unienv"。
pub fn dispatch(service: &str, operation: &str, payload: Option<&Value>) -> Result<Value, String> {
    if service != "unienv" {
        return Err(format!("unknown trusted service: {service}"));
    }
    match operation {
        "activate" | "deactivate" => Ok(Value::Null),
        "message" => {
            let payload =
                payload.ok_or_else(|| "message operation requires payload".to_string())?;
            Ok(handle_message(payload))
        }
        _ => Err(format!("unknown trusted operation: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_tools_returns_meta() {
        let out = dispatch("unienv", "message", Some(&json!({ "type": "listTools" }))).unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 5);
        assert!(arr.iter().any(|t| t["id"] == "python"));
    }

    #[test]
    fn list_versions_known_tool() {
        let out = dispatch(
            "unienv",
            "message",
            Some(&json!({ "type": "listVersions", "tool": "go" })),
        )
        .unwrap();
        let arr = out.as_array().unwrap();
        assert!(arr.contains(&json!("1.26.5")));
    }

    #[test]
    fn list_versions_unknown_tool_errors() {
        let out = dispatch(
            "unienv",
            "message",
            Some(&json!({ "type": "listVersions", "tool": "ruby" })),
        )
        .unwrap();
        assert_eq!(out["code"], "unknown-tool");
    }

    #[test]
    fn list_combos_returns_builtin() {
        let out = dispatch("unienv", "message", Some(&json!({ "type": "listCombos" }))).unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 5);
        assert!(arr.iter().any(|c| c["id"] == "python-fullstack"));
    }

    #[test]
    fn detect_missing_returns_not_installed() {
        let out = dispatch("unienv", "message", Some(&json!({ "type": "detect", "tool": "go", "installRoot": "C:\\__nonexistent_unienv_test__" }))).unwrap();
        assert_eq!(out["installed"], false);
    }

    #[test]
    fn install_not_supported_yet() {
        let out = dispatch(
            "unienv",
            "message",
            Some(&json!({ "type": "install", "tool": "go", "version": "1.26.5" })),
        )
        .unwrap();
        assert_eq!(out["code"], "not-supported-yet");
    }

    #[test]
    fn wrong_service_rejected() {
        assert!(dispatch("other", "message", None).is_err());
    }
}
