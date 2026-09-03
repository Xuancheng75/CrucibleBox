//! Windows-native marketplace transport.
//!
//! BITS owns the network transfer and retry/resume policy, while the job is
//! explicitly configured for direct HTTPS without a proxy;
//! the host still owns the destination, size/digest verification and plugin
//! installation. The existing ureq path remains the compatibility fallback
//! for systems where the BITS service is unavailable.

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
pub fn download_with_bits(
    url: &str,
    partial: &Path,
    artifact: &str,
    expected_size: u64,
    priority_foreground: bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    use std::thread;
    use std::time::Duration;
    use windows::core::{GUID, HSTRING};
    use windows::Win32::Networking::BackgroundIntelligentTransferService::{
        BackgroundCopyManager, IBackgroundCopyJob, BG_JOB_PRIORITY_FOREGROUND,
        BG_JOB_PRIORITY_NORMAL, BG_JOB_PROGRESS, BG_JOB_PROXY_USAGE_NO_PROXY,
        BG_JOB_STATE_ACKNOWLEDGED, BG_JOB_STATE_CANCELLED, BG_JOB_STATE_ERROR,
        BG_JOB_STATE_TRANSFERRED, BG_JOB_STATE_TRANSIENT_ERROR, BG_JOB_TYPE_DOWNLOAD,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED,
    };

    if url.is_empty() || !url.starts_with("https://") {
        return Err("BITS 只允许 HTTPS 下载地址".into());
    }
    if expected_size == 0 {
        return Err("BITS 需要目录提供有效的插件包大小".into());
    }
    if partial.exists() {
        return Err("BITS 不接管已有断点文件".into());
    }
    if let Some(parent) = partial.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建下载目录失败：{error}"))?;
    }

    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        if init.is_err() {
            return Err(format!("初始化 BITS COM 失败：{init:?}"));
        }
        let result = (|| -> Result<(), String> {
            let manager = CoCreateInstance::<_, windows::Win32::Networking::BackgroundIntelligentTransferService::IBackgroundCopyManager>(
                &BackgroundCopyManager,
                None,
                CLSCTX_LOCAL_SERVER,
            )
            .map_err(|error| format!("创建 BITS 管理器失败：{error}"))?;
            let display_name = HSTRING::from(format!("CrucibleBox marketplace {artifact}"));
            let mut job_id = GUID::zeroed();
            let mut job: Option<IBackgroundCopyJob> = None;
            manager
                .CreateJob(&display_name, BG_JOB_TYPE_DOWNLOAD, &mut job_id, &mut job)
                .map_err(|error| format!("创建 BITS 下载任务失败：{error}"))?;
            let job = job.ok_or_else(|| "BITS 未返回下载任务".to_string())?;
            let remote = HSTRING::from(url);
            let local = HSTRING::from(partial.to_string_lossy().as_ref());
            job.AddFile(&remote, &local)
                .map_err(|error| format!("加入 BITS 下载文件失败：{error}"))?;
            job.SetProxySettings(
                BG_JOB_PROXY_USAGE_NO_PROXY,
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
            )
            .map_err(|error| format!("设置 BITS 直连失败：{error}"))?;
            job.SetPriority(if priority_foreground {
                BG_JOB_PRIORITY_FOREGROUND
            } else {
                BG_JOB_PRIORITY_NORMAL
            })
            .map_err(|error| format!("设置 BITS 下载优先级失败：{error}"))?;
            job.SetNoProgressTimeout(120)
                .map_err(|error| format!("设置 BITS 超时失败：{error}"))?;
            job.Resume()
                .map_err(|error| format!("启动 BITS 下载失败：{error}"))?;

            let mut last_transferred = 0_u64;
            loop {
                let state = job
                    .GetState()
                    .map_err(|error| format!("读取 BITS 状态失败：{error}"))?;
                let mut progress = BG_JOB_PROGRESS::default();
                job.GetProgress(&mut progress)
                    .map_err(|error| format!("读取 BITS 进度失败：{error}"))?;
                let transferred = progress.BytesTransferred.min(expected_size);
                if transferred != last_transferred {
                    last_transferred = transferred;
                    on_progress(transferred, expected_size);
                }
                if state == BG_JOB_STATE_TRANSFERRED || state == BG_JOB_STATE_ACKNOWLEDGED {
                    job.Complete()
                        .map_err(|error| format!("提交 BITS 下载结果失败：{error}"))?;
                    on_progress(expected_size, expected_size);
                    return Ok(());
                }
                if state == BG_JOB_STATE_ERROR || state == BG_JOB_STATE_TRANSIENT_ERROR {
                    let _ = job.Cancel();
                    return Err("BITS 下载失败".into());
                }
                if state == BG_JOB_STATE_CANCELLED {
                    return Err("BITS 下载已取消".into());
                }
                thread::sleep(Duration::from_millis(250));
            }
        })();
        CoUninitialize();
        result
    }
}

#[cfg(not(windows))]
pub fn download_with_bits(
    _url: &str,
    _partial: &std::path::Path,
    _artifact: &str,
    _expected_size: u64,
    _priority_foreground: bool,
    _on_progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    Err("BITS 仅支持 Windows".into())
}
