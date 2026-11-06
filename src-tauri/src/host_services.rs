//! Explicit composition of Next 3.0 platform ports and task runtime.
use crate::next2_host::platform_service::Platform;
use std::sync::Arc;
pub struct Services {
    pub runtime: Arc<crate::task_runtime::TaskRuntime>,
    pub platform: Platform,
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
            platform: Platform::new(app_handle),
        }
    }
}
