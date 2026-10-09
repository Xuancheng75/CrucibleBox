//! One host clipboard reader shared by active subscribers. An empty registry
//! parks the worker without touching clipboard contents.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

#[derive(Default)]
struct MonitorState {
    subscribers: HashMap<String, Subscriber>,
    shutdown: bool,
}

struct Subscriber {
    emitter: Emitter,
    baseline: Option<String>,
    excluded_apps: Vec<String>,
}

impl Subscriber {
    fn accepts(&self, foreground: Option<&str>) -> bool {
        self.excluded_apps.is_empty()
            || foreground.is_some_and(|name| {
                !self
                    .excluded_apps
                    .iter()
                    .any(|excluded| excluded.eq_ignore_ascii_case(name))
            })
    }
}

impl MonitorState {
    fn observe(&mut self, text: &str, foreground: Option<&str>) -> Vec<(String, Emitter)> {
        self.subscribers
            .iter_mut()
            .filter_map(|(id, subscriber)| {
                if !subscriber.accepts(foreground) {
                    subscriber.baseline = None;
                    return None;
                }
                let changed = subscriber
                    .baseline
                    .as_deref()
                    .is_some_and(|old| old != text);
                subscriber.baseline = Some(text.to_string());
                (changed && !text.is_empty()).then(|| (id.clone(), subscriber.emitter.clone()))
            })
            .collect()
    }
}

struct MonitorRegistry {
    state: Arc<(Mutex<MonitorState>, Condvar)>,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

fn registry() -> &'static MonitorRegistry {
    static REGISTRY: OnceLock<MonitorRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| MonitorRegistry {
        state: Arc::new((Mutex::new(MonitorState::default()), Condvar::new())),
        handle: Mutex::new(None),
    })
}

pub fn start(plugin_id: &str, emitter: Emitter, excluded_apps: Vec<String>) -> Result<(), String> {
    let registry = registry();
    let mut handle = registry.handle.lock().map_err(|error| error.to_string())?;
    if handle.is_none() {
        let state = registry.state.clone();
        *handle = Some(
            std::thread::Builder::new()
                .name("clipboard-monitor".into())
                .spawn(move || monitor_loop(state))
                .map_err(|error| error.to_string())?,
        );
    }
    let (state, changed) = &*registry.state;
    state
        .lock()
        .map_err(|error| error.to_string())?
        .subscribers
        .insert(
            plugin_id.to_string(),
            Subscriber {
                emitter,
                baseline: None,
                excluded_apps,
            },
        );
    changed.notify_one();
    Ok(())
}

pub fn stop(plugin_id: &str) {
    let (state, _) = &*registry().state;
    if let Ok(mut state) = state.lock() {
        state.subscribers.remove(plugin_id);
    }
}

pub fn stop_all() {
    let registry = registry();
    let (state, changed) = &*registry.state;
    if let Ok(mut state) = state.lock() {
        state.subscribers.clear();
        state.shutdown = true;
        changed.notify_one();
    }
    if let Ok(mut handle) = registry.handle.lock() {
        if let Some(handle) = handle.take() {
            let _ = handle.join();
        }
    }
}

fn monitor_loop(shared: Arc<(Mutex<MonitorState>, Condvar)>) {
    let (state, changed) = &*shared;
    loop {
        let deliveries = {
            let mut guard = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            while guard.subscribers.is_empty() && !guard.shutdown {
                guard = changed
                    .wait(guard)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if guard.shutdown {
                return;
            }
            // Serialize reading with pause/removal: a captured subscriber list
            // must not trigger a new clipboard read after the last subscriber stops.
            let foreground = if guard
                .subscribers
                .values()
                .any(|subscriber| !subscriber.excluded_apps.is_empty())
            {
                foreground_application()
            } else {
                None
            };
            for subscriber in guard.subscribers.values_mut() {
                if !subscriber.accepts(foreground.as_deref()) {
                    subscriber.baseline = None;
                }
            }
            if !guard
                .subscribers
                .values()
                .any(|subscriber| subscriber.accepts(foreground.as_deref()))
            {
                None
            } else {
                arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut clipboard| clipboard.get_text().ok())
                    .map(|text| (guard.observe(&text, foreground.as_deref()), text))
            }
        };
        if let Some((subscribers, text)) = deliveries {
            for (plugin_id, emitter) in subscribers {
                emitter(
                    "plugin:clipboard",
                    serde_json::json!({ "pluginId": plugin_id, "text": text }),
                );
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(windows)]
fn foreground_application() -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    let mut id = 0;
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut id);
    }
    if id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(process);
    }
    if success == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_string)
}

#[cfg(not(windows))]
fn foreground_application() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscriber() -> Subscriber {
        Subscriber {
            emitter: Arc::new(|_, _| {}),
            baseline: None,
            excluded_apps: Vec::new(),
        }
    }

    #[test]
    fn excluded_foreground_resets_baseline_without_recording_paused_content() {
        let mut state = MonitorState::default();
        let mut filtered = subscriber();
        filtered.excluded_apps = vec!["Secrets.exe".into()];
        state.subscribers.insert("filtered".into(), filtered);
        state.subscribers.insert("unfiltered".into(), subscriber());
        assert!(state.observe("initial", Some("Editor.exe")).is_empty());
        let deliveries = state.observe("secret", Some("SECRETS.EXE"));
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].0, "unfiltered");
        assert!(state.observe("secret", Some("Editor.exe")).is_empty());
        assert_eq!(state.observe("new copy", Some("Editor.exe")).len(), 2);
        assert!(!state.subscribers["filtered"].accepts(None));
        assert!(state.subscribers["unfiltered"].accepts(None));
    }

    #[test]
    fn resumed_subscriber_does_not_receive_content_from_its_pause() {
        let mut state = MonitorState::default();
        state
            .subscribers
            .insert("always-active".into(), subscriber());
        state.subscribers.insert("paused".into(), subscriber());
        assert!(state.observe("initial", None).is_empty());
        state.subscribers.remove("paused");
        assert_eq!(state.observe("during pause", None).len(), 1);
        state.subscribers.insert("paused".into(), subscriber());
        assert!(state.observe("during pause", None).is_empty());
        assert_eq!(state.observe("new copy", None).len(), 2);
        assert!(state.observe("new copy", None).is_empty());
    }

    #[test]
    fn clearing_clipboard_updates_baseline_without_emitting_empty_record() {
        let mut state = MonitorState::default();
        state.subscribers.insert("demo".into(), subscriber());
        assert!(state.observe("same", None).is_empty());
        assert!(state.observe("", None).is_empty());
        assert_eq!(state.observe("same", None).len(), 1);
    }
}
