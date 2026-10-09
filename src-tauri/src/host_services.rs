//! Host composition state injected into backend workers; no service initialization globals.
use std::sync::Arc;
pub struct Services {
    pub results: crate::next_results::Results,
    pub runtime: Arc<crate::task_runtime::TaskRuntime>,
    pub platform: crate::platform_service::Platform,
    pub document: crate::document_engine_service::Service,
    pub unienv: crate::unienv_service::Service,
    pub archive: crate::archive_service::Service,
    pub process_tasks: Arc<crate::unienv_task::TaskManager>,
}
impl Services {
    #[cfg(test)]
    pub fn new(runtime: Arc<crate::task_runtime::TaskRuntime>) -> Self {
        Self::with_document_worker(runtime, None, None, None, None)
    }
    pub fn with_document_worker(
        runtime: Arc<crate::task_runtime::TaskRuntime>,
        worker: Option<Arc<crate::ocr_worker::OcrWorkerManager>>,
        resources: Option<std::path::PathBuf>,
        app_handle: Option<tauri::AppHandle>,
        document_runtime_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            results: crate::next_results::Results::default(),
            runtime: runtime.clone(),
            platform: crate::platform_service::Platform::new(app_handle),
            document: crate::document_engine_service::Service::with_worker(
                runtime.clone(),
                worker,
                resources,
                document_runtime_root,
            ),
            unienv: crate::unienv_service::Service::new(runtime.clone()),
            archive: crate::archive_service::Service::new(runtime.clone()),
            process_tasks: Arc::new(crate::unienv_task::TaskManager::with_runtime(
                "plugin-process",
                runtime,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separate_compositions_do_not_share_process_task_control() {
        let runtime = crate::task_runtime::TaskRuntime::memory();
        let first = Services::new(runtime.clone());
        let second = Services::new(crate::task_runtime::TaskRuntime::memory());
        let id = first
            .process_tasks
            .start(
                "plugin-process:diary",
                Box::new(|ctx| loop {
                    ctx.check_cancelled()?;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }),
            )
            .unwrap();
        assert!(second.process_tasks.get(&id).is_none());
        assert!(!second.process_tasks.cancel(&id));
        assert!(first.process_tasks.cancel(&id));
        for _ in 0..200 {
            if runtime.get("plugin-process", &id).unwrap()["status"] == "cancelled" {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("executor failed to stop after cancellation");
    }
}
