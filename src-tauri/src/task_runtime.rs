//! Tauri composition imports the standalone task runtime.
pub use cruciblebox_task_runtime::{Context, TaskRuntime};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    fn wait(runtime: &TaskRuntime, owner: &str, id: &str, status: &str) -> Value {
        for _ in 0..300 {
            let value = runtime.get(owner, id).unwrap();
            if value["status"] == status {
                return value;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out: {:?}", runtime.get(owner, id));
    }
    #[test]
    fn publication_does_not_block_unrelated_tasks_and_cancel_all_keeps_other_owners() {
        let runtime = TaskRuntime::memory();
        let (began_tx, began_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let id = runtime
            .start(
                "document-engine",
                "convert",
                Box::new(move |ctx| {
                    ctx.commit_result(|| {
                        began_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(json!({"outputPath":"C:/document.md"}))
                    })
                }),
            )
            .unwrap();
        began_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        let other = runtime.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = other.run_sync("unienv", "installation", Some("other-install"), |_| {
                Ok(json!({"kind":"install"}))
            });
            done_tx.send(result).unwrap();
        });
        let completed_before_publish = done_rx.recv_timeout(std::time::Duration::from_millis(500));
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(
            completed_before_publish.unwrap().is_ok(),
            "unrelated task waited for file publication"
        );
        wait(&runtime, "document-engine", &id, "succeeded");

        let install = Arc::new(crate::unienv_task::TaskManager::default());
        install.set_runtime(runtime.clone()).unwrap();
        let archive = Arc::new(crate::unienv_task::TaskManager::new("archive-extractor"));
        archive.set_runtime(runtime.clone()).unwrap();
        let release = Arc::new(AtomicBool::new(false));
        let keep = release.clone();
        let archive_id = archive
            .start(
                "archive-extraction",
                Box::new(move |ctx| {
                    while !keep.load(Ordering::SeqCst) {
                        ctx.check_cancelled()?;
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Ok(Value::Null)
                }),
            )
            .unwrap();
        wait(&runtime, "archive-extractor", &archive_id, "running");
        assert_eq!(install.cancel_all_active(), 0);
        assert!(!install.cancel(&archive_id));
        assert!(install.get(&archive_id).is_none());
        release.store(true, Ordering::SeqCst);
        wait(&runtime, "archive-extractor", &archive_id, "succeeded");
    }
}
