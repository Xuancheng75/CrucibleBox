// 应用级常量（1.8.1）

/// 插件侧 db.query/db.execute 直通命令的内部令牌。
/// 1.8.2 sidecar 落地后由 sidecar 携带该令牌并经权限守卫；渲染进程不可获得。
pub const INTERNAL_DB_TOKEN: &str = env!("CARGO_PKG_NAME");
