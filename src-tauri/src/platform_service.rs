//! Tauri platform adapter state, explicitly owned by the host composition.
use serde_json::Value;
use std::sync::Mutex;
use std::time::Instant;
pub struct Platform {
    pub app_handle: Option<tauri::AppHandle>,
    pub system_info_cache: Mutex<Option<(Instant, Value)>>,
}
impl Platform {
    pub fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        Self {
            app_handle,
            system_info_cache: Mutex::new(None),
        }
    }
}
