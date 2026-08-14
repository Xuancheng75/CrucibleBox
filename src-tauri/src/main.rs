// CrucibleBox Tauri 骨架（1.8.0）
// 目标：验证 Tauri 2 + WebView2 壳 + 最小 React 前端可运行，并提供进程内存自检命令供 P4 基准。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::process;

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
    // Windows: 用 OpenProcess + GetProcessMemoryInfo 精确读 working set
    #[cfg(windows)]
    {
        use std::mem::size_of;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_INFORMATION,
        };

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
        return Ok(ProcessMemory {
            working_set_kib: (counters.WorkingSetSize / 1024) as u64,
            private_kib: (counters.PrivateUsage / 1024) as u64,
            pid,
        });
    }
    #[cfg(not(windows))]
    {
        Err("memory probe is Windows-only".into())
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_process_memory])
        .run(tauri::generate_context!())
        .expect("error while running CrucibleBox");
}
