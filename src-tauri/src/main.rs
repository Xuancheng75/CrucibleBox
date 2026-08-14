// CrucibleBox Tauri 主进程（1.8.4）
// 启动序（对等 electron/main.ts mainLoop 的 DB 相关步骤）：
//   1) L3 数据路径迁移（%APPDATA%\openbox → %APPDATA%\cruciblebox）
//   2) 打开 DB（WAL + v3 迁移 + 日志清理）；失败则安全退出
//   3) manage 状态 + 注册命令

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend_process;
mod commands;
mod data_dir;
mod db;
mod envelope_host;
mod permissions;
mod plugin_protocol;
mod plugin_session;
mod rand_token;

use serde::Serialize;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};
use tauri::Manager;

/// 进程内存信息（供 P4 内存 A/B 基准从前端查询）
#[derive(Serialize)]
struct ProcessMemory {
    working_set_kib: u64,
    private_kib: u64,
    pid: u32,
}

#[tauri::command]
fn get_process_memory() -> Result<ProcessMemory, String> {
    let pid = process::id();
    #[cfg(windows)]
    {
        use std::mem::size_of;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};

        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid) };
        if handle.is_null() {
            return Err(format!("OpenProcess failed for pid {pid}"));
        }
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            GetProcessMemoryInfo(
                handle,
                &mut counters as *mut _ as *mut _,
                size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return Err("GetProcessMemoryInfo failed".into());
        }
        Ok(ProcessMemory {
            working_set_kib: (counters.WorkingSetSize / 1024) as u64,
            private_kib: (counters.PrivateUsage / 1024) as u64,
            pid,
        })
    }
    #[cfg(not(windows))]
    {
        Err("memory probe is Windows-only".into())
    }
}

fn db_path_in(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("data").join("openbox.db")
}

/// Windows 错误对话框（对等 electron 的 dialog.showErrorBox）；
/// 非 Windows 平台降级为 stderr。
#[cfg(windows)]
fn show_fatal_error(message: &str) -> ! {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let wide: Vec<u16> = OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            wide.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
    std::process::exit(1)
}

#[cfg(not(windows))]
fn show_fatal_error(message: &str) -> ! {
    eprintln!("[FATAL] {message}");
    std::process::exit(1)
}

fn main() {
    tauri::Builder::default()
        // tauri-plugin-updater（1.8.4：JSON 清单 + 强制签名；密钥走 CI secret）
        .plugin(tauri_plugin_updater::Builder::new().build())
        // 插件 renderer 自定义协议（1.8.3）：http://cruciblebox-plugin.localhost/<token>/<res>
        // handler 经 app_handle.state 取 registry（setup 中 manage）
        .register_uri_scheme_protocol(plugin_session::PLUGIN_RENDERER_SCHEME, |ctx, request| {
            let protocol = ctx
                .app_handle()
                .state::<Arc<plugin_protocol::ProtocolContext>>();
            plugin_protocol::handle_protocol(&protocol, request.uri().to_string())
        })
        .setup(|app| {
            // 1) L3 数据路径迁移。
            //    策略：迁移失败是数据完整性风险（checkpoint 失败/IO 错误），中止启动并给
            //    用户可见错误框（源数据未动，可关闭旧实例重试）；无旧数据/已迁移则跳过。
            let migration = match data_dir::migrate(None) {
                Ok(m) => m,
                Err(err) => show_fatal_error(&format!(
                    "数据库迁移失败：{}\n\n源数据未受影响，请关闭旧版应用后重试。",
                    err
                )),
            };
            let data_dir = migration.target.clone();
            if migration.migrated {
                eprintln!(
                    "[DB] L3 data migration: {} -> {} ({} entries copied, checkpoint={})",
                    migration
                        .source
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    migration.target.display(),
                    migration.copied_entries.len(),
                    migration.checkpointed
                );
            }

            // 2) 确保数据目录存在（C1：全新安装无 %APPDATA%\openbox 也必须有 data/ 目录）
            if let Err(err) = std::fs::create_dir_all(data_dir.join("data")) {
                show_fatal_error(&format!("无法创建数据目录：{}", err));
            }

            // 3) 打开 DB；失败则安全退出（对等 electron main.ts 行为，含用户提示）
            let db_path = db_path_in(&data_dir);
            let db = match db::Db::open(&db_path) {
                Ok(db) => db,
                Err(err) => show_fatal_error(&format!(
                    "数据库打开失败：{}\n\n数据库未被修改，应用将安全退出。",
                    err
                )),
            };
            eprintln!(
                "[DB] engine: rusqlite bundled (WAL) @ {}",
                db_path.display()
            );

            // 4) manage 状态（commands 以 State<Arc<Mutex<Db>>> 访问）+ 数据目录 + backend 管理器
            let db = Arc::new(Mutex::new(db));
            let backend = backend_process::BackendProcessManager::new(db.clone());
            app.manage(db.clone());
            app.manage(data_dir);
            app.manage(backend.clone());

            // 5) 插件 renderer 会话 registry（协议 handler 经 state 访问）
            let registry = Arc::new(Mutex::new(plugin_session::RendererSessionRegistry::new(
                plugin_session::DEFAULT_TTL,
            )));
            app.manage(registry.clone());
            app.manage(Arc::new(plugin_protocol::ProtocolContext {
                registry,
                owner_label: "main".into(),
            }));

            // 6) 退出清理：kill 全部存活 backend 进程（对等 PluginManager deactivateAll）
            //    经 AppHandle.state 访问（run 回调在主线程、事件驱动触发）
            let _ = app.handle();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_process_memory,
            commands::settings_get,
            commands::settings_set,
            commands::settings_get_all,
            commands::app_get_version,
            commands::app_get_platform,
            commands::plugin_list,
            commands::plugin_get,
            commands::plugin_enable,
            commands::plugin_disable,
            commands::plugin_reorder,
            commands::plugin_update_config,
            commands::plugin_get_logs,
            commands::plugin_clear_logs,
            commands::plugin_uninstall,
            commands::db_status,
            commands::create_renderer_session,
            commands::dispose_renderer_session,
            commands::plugin_send_message,
        ])
        .build(tauri::generate_context!())
        .expect("error while building CrucibleBox")
        .run(|app_handle, event| {
            // RunEvent::Exit：清理全部存活 backend 进程
            if let tauri::RunEvent::Exit = event {
                if let Some(backend) =
                    app_handle.try_state::<Arc<backend_process::BackendProcessManager>>()
                {
                    backend.kill_all();
                }
            }
        });
}
