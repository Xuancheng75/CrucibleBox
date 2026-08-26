// 权限守卫（1.9.2-a，对等 plugin-system/PermissionGuard.ts + shared/types/permissions.ts）
// 语义：权限声明是 SDK 能力门控，非安全边界（信任模型 A）；
// 宿主侧逐调用校验是唯一权威边界（插件可绕过 __buildCtx 直调 __hostRequest）。

use std::collections::HashSet;

/// 对等 shared/types/permissions.ts 的 15 个权限串
pub const ALL_PERMISSIONS: &[&str] = &[
    "database:read",
    "database:write",
    "storage:read",
    "storage:write",
    "shell:exec",
    "network:fetch",
    "notification",
    "clipboard",
    "dialog",
    "shortcut",
    "file:read",
    "file:write",
    "theme:write",
    "trusted:unienv",
    "trusted:document-engine",
];

pub const DATABASE_READ: &str = "database:read";
pub const DATABASE_WRITE: &str = "database:write";
pub const STORAGE_READ: &str = "storage:read";
pub const STORAGE_WRITE: &str = "storage:write";
pub const NETWORK_FETCH: &str = "network:fetch";
pub const NOTIFICATION: &str = "notification";
pub const DIALOG: &str = "dialog";
pub const SHORTCUT: &str = "shortcut";
pub const FILE_READ: &str = "file:read";
pub const FILE_WRITE: &str = "file:write";
// theme:write 由 renderer RPC 侧（PluginFrameBridge）校验；host 方法面暂无对应，
// 保留常量供 1.9.2-c 前端接线时使用
#[allow(dead_code)]
pub const THEME_WRITE: &str = "theme:write";
pub const TRUSTED_UNIENV: &str = "trusted:unienv";
pub const TRUSTED_DOCUMENT_ENGINE: &str = "trusted:document-engine";

pub struct PermissionGuard {
    granted: HashSet<&'static str>,
}

impl PermissionGuard {
    /// 从插件 manifest permissions JSON 解析（过滤未知权限，对等 parsePermissions）
    pub fn from_json(raw: &str) -> Self {
        let perms: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
        Self::parse(&perms)
    }

    pub fn parse(raw: &[String]) -> Self {
        let granted = raw
            .iter()
            .filter_map(|p| ALL_PERMISSIONS.iter().find(|known| **known == p).copied())
            .collect();
        PermissionGuard { granted }
    }

    pub fn has(&self, permission: &str) -> bool {
        self.granted.contains(permission)
    }

    /// trusted.invoke 门控：宿主固定可信服务（UniEnv / Document Engine 等）共用
    /// 同一 host 方法，故接受任一 trusted:* 权限即可。
    pub fn assert_trusted_service(&self) -> Result<(), String> {
        if self.has(TRUSTED_UNIENV) || self.has(TRUSTED_DOCUMENT_ENGINE) {
            Ok(())
        } else {
            Err(
                "Permission denied: trusted service (trusted:unienv or trusted:document-engine)"
                    .into(),
            )
        }
    }

    /// 断言权限；拒绝返回 NOT_ALLOWED 错误文本（对等 assert 抛错语义）
    pub fn assert(&self, permission: &str) -> Result<(), String> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(format!("Permission denied: {permission}"))
        }
    }
}

/// host 方法 → 所需权限映射（1.9.2-a 实现面；None = 无权限门禁，如日志/事件天然限本插件）
pub fn permission_for_host_method(method: &str) -> Option<&'static str> {
    match method {
        "db.query" => Some(DATABASE_READ),
        "db.execute" => Some(DATABASE_WRITE),
        "storage.get" | "storage.list" => Some(STORAGE_READ),
        "storage.set" | "storage.delete" | "storage.batch" => Some(STORAGE_WRITE),
        "notification.show" => Some(NOTIFICATION),
        "dialog.open" => Some(DIALOG),
        "network.fetch" => Some(NETWORK_FETCH),
        "file.read" => Some(FILE_READ),
        "file.write" => Some(FILE_WRITE),
        "shortcut.register" | "shortcut.unregister" => Some(SHORTCUT),
        "trusted.invoke" => Some(TRUSTED_UNIENV),
        // 无权限门禁：log.write、event.*（日志与事件天然按插件隔离）
        _ => None,
    }
}

/// 判断 host 方法是否为 1.9.2-a 已实现面（未实现 → NOT_ALLOWED）
pub fn is_host_method_implemented(method: &str) -> bool {
    matches!(
        method,
        "db.query"
            | "db.execute"
            | "storage.get"
            | "storage.set"
            | "storage.delete"
            | "storage.list"
            | "storage.batch"
            | "log.write"
            | "event.emit"
            | "event.subscribe"
            | "event.unsubscribe"
            | "trusted.invoke"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn parse_filters_unknown_permissions() {
        let guard = PermissionGuard::parse(&[
            "storage:read".into(),
            "storage:write".into(),
            "totally:unknown".into(),
        ]);
        assert!(guard.has(STORAGE_READ));
        assert!(guard.has(STORAGE_WRITE));
        assert!(!guard.has(NETWORK_FETCH));
        // unknown 被过滤，granted 集只有 2 个
        assert_eq!(guard.granted.len(), 2);
    }

    #[test]
    fn assert_granted_and_denied() {
        let guard = PermissionGuard::parse(&["storage:read".into()]);
        assert!(guard.assert(STORAGE_READ).is_ok());
        assert!(guard.assert(STORAGE_WRITE).is_err());
        assert!(guard.assert(NETWORK_FETCH).is_err());
    }

    #[test]
    fn permission_mapping() {
        assert_eq!(permission_for_host_method("db.query"), Some(DATABASE_READ));
        assert_eq!(
            permission_for_host_method("storage.set"),
            Some(STORAGE_WRITE)
        );
        assert_eq!(
            permission_for_host_method("trusted.invoke"),
            Some(TRUSTED_UNIENV)
        );
        assert_eq!(permission_for_host_method("log.write"), None);
        assert_eq!(permission_for_host_method("event.subscribe"), None);
    }

    #[test]
    fn implemented_surface() {
        assert!(is_host_method_implemented("storage.get"));
        assert!(is_host_method_implemented("log.write"));
        assert!(!is_host_method_implemented("dialog.open"));
        assert!(!is_host_method_implemented("network.fetch"));
        assert!(is_host_method_implemented("trusted.invoke"));
    }

    #[test]
    fn from_json_parses_manifest_column() {
        let guard = PermissionGuard::from_json(r#"["notification","storage:read"]"#);
        assert!(guard.has(NOTIFICATION));
        assert!(guard.has(STORAGE_READ));
        assert!(!guard.has(DIALOG));
    }

    #[test]
    fn json_value_is_serde_compatible() {
        // 确保与 DB plugins.permissions JSON 列解析路径一致
        let raw = Value::String(r#"["storage:read"]"#.into());
        let guard = PermissionGuard::from_json(raw.as_str().unwrap());
        assert!(guard.has(STORAGE_READ));
    }
}
