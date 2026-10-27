//! Explicit composition of API 6 platform ports and task runtime.
use std::sync::Arc;
pub struct Services {
    pub runtime: Arc<crate::task_runtime::TaskRuntime>,
    pub platform: crate::platform_service::Platform,
}
impl Services {
    #[cfg(test)]
    pub fn new(runtime: Arc<crate::task_runtime::TaskRuntime>) -> Self {
        Self::with_platform(runtime, None)
    }
    pub fn with_platform(
        runtime: Arc<crate::task_runtime::TaskRuntime>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        Self {
            runtime,
            platform: crate::platform_service::Platform::new(app_handle),
        }
    }
}
