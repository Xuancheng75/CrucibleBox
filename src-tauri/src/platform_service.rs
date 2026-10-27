//! Tauri platform handle owned by the host composition.
pub struct Platform {
    pub app_handle: Option<tauri::AppHandle>,
}
impl Platform {
    pub fn new(app_handle: Option<tauri::AppHandle>) -> Self {
        Self { app_handle }
    }
}
