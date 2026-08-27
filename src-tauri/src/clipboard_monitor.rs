// 剪贴板监控（1.9.17）：宿主侧线程轮询系统剪贴板，变化时经 emitter 广播
// plugin:clipboard 事件，前端转发至插件 onMessage。
// 解决 sidecar quickjs-ng 无 setInterval 的问题。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

struct MonitorEntry {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

struct MonitorRegistry {
    entries: HashMap<String, MonitorEntry>,
}

fn registry() -> &'static Mutex<MonitorRegistry> {
    static REG: std::sync::OnceLock<Mutex<MonitorRegistry>> = std::sync::OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(MonitorRegistry {
            entries: HashMap::new(),
        })
    })
}

pub fn start(plugin_id: &str, emitter: Emitter) -> Result<(), String> {
    let mut reg = registry().lock().map_err(|e| e.to_string())?;
    if reg.entries.contains_key(plugin_id) {
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let pid = plugin_id.to_string();
    let handle = std::thread::Builder::new()
        .name(format!("clipboard-mon-{pid}"))
        .spawn(move || monitor_loop(&pid, stop_clone, emitter))
        .map_err(|e| e.to_string())?;
    reg.entries.insert(
        plugin_id.to_string(),
        MonitorEntry {
            stop,
            handle: Some(handle),
        },
    );
    Ok(())
}

pub fn stop(plugin_id: &str) {
    let entry = {
        let mut reg = match registry().lock() {
            Ok(r) => r,
            Err(_) => return,
        };
        reg.entries.remove(plugin_id)
    };
    if let Some(mut entry) = entry {
        entry.stop.store(true, Ordering::SeqCst);
        if let Some(h) = entry.handle.take() {
            let _ = h.join();
        }
    }
}

pub fn stop_all() {
    let entries = {
        let mut reg = match registry().lock() {
            Ok(r) => r,
            Err(_) => return,
        };
        std::mem::take(&mut reg.entries)
    };
    for (_, mut entry) in entries {
        entry.stop.store(true, Ordering::SeqCst);
        if let Some(h) = entry.handle.take() {
            let _ = h.join();
        }
    }
}

fn monitor_loop(plugin_id: &str, stop: Arc<AtomicBool>, emitter: Emitter) {
    let mut last_text = String::new();
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() && text != last_text {
                    last_text = text.clone();
                    emitter(
                        "plugin:clipboard",
                        serde_json::json!({ "pluginId": plugin_id, "text": text }),
                    );
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
