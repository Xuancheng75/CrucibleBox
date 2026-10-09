//! Local lifecycle trace. It contains only stage names and plugin identifiers,
//! never document text, paths, request bodies, or credentials.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn initialize(data_dir: &Path) {
    let path = data_dir.join("logs").join("fault-history.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = LOG_PATH.set(path);
    record("startup", None);
}

pub fn record(stage: &str, plugin_id: Option<&str>) {
    let Some(path) = LOG_PATH.get() else { return };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let plugin = plugin_id.unwrap_or("-");
        let _ = writeln!(
            file,
            "{timestamp}\t{}\t{stage}\t{plugin}",
            env!("CARGO_PKG_VERSION")
        );
    }
}

pub fn read() -> Result<(String, String), String> {
    let path = LOG_PATH
        .get()
        .ok_or_else(|| "故障记录尚未初始化".to_string())?;
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines = text.lines().rev().take(500).collect::<Vec<_>>();
    Ok((
        path.to_string_lossy().into_owned(),
        lines.into_iter().rev().collect::<Vec<_>>().join("\n"),
    ))
}
