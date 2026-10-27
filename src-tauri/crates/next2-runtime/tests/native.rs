#![cfg(all(windows, feature = "test-backend"))]
use cruciblebox_next2_runtime::{package::*, worker::Worker};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn package(root: &Path, wrong_wire: bool) -> Value {
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_next2-test-backend"),
        root.join("bin/backend.exe"),
    )
    .unwrap();
    fs::write(root.join("dist/renderer.js"), b"export {};").unwrap();
    let mut files = vec!["bin/backend.exe", "dist/renderer.js"];
    if wrong_wire {
        fs::write(root.join("wrong-wire.txt"), b"wrong protocol").unwrap();
        files.push("wrong-wire.txt");
    }
    let inventory: Vec<_> = files
        .iter()
        .map(|name| {
            let data = fs::read(root.join(name)).unwrap();
            json!({"path":name,"bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(&data))})
        })
        .collect();
    let mut manifest: Value =
        serde_json::from_str(include_str!("../../../../contracts/next2/fixtures.json")).unwrap();
    manifest = manifest["native"].clone();
    manifest["files"] = json!(inventory);
    fs::write(root.join("plugin.json"), manifest.to_string()).unwrap();
    manifest
}
fn approve(root: &Path) -> ApprovedPlugin {
    let inspection = inspect_plugin(root, &[], "2.2.0-alpha.1").unwrap();
    let digest = inspection.manifest_sha256();
    inspection
        .approve(Approval {
            manifest_sha256: digest,
            accept_native_code: true,
            accept_unsigned_local: true,
            accept_permissions: true,
        })
        .unwrap()
}
fn stopped(pid: u32) -> bool {
    use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if process.is_null() {
            return true;
        }
        let status = WaitForSingleObject(process, 0);
        CloseHandle(process);
        status == 0
    }
}
#[test]
fn actual_native_input_identity_progress_cancel_crash_and_exit() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), false);
    let approved = approve(root.path());
    let data = tempfile::tempdir().unwrap();
    let mut worker = Worker::start(&approved, data.path(), &[]).unwrap();
    let pid = worker.pid();
    let flag = AtomicBool::new(false);
    assert_eq!(
        worker
            .call(
                "test-client",
                "undefined-business-method",
                json!({}),
                None,
                &flag,
                |_| unreachable!()
            )
            .unwrap_err()
            .code,
        "METHOD_UNAVAILABLE"
    );
    assert_eq!(
        worker
            .call(
                "test-client",
                "sum",
                json!({"numbers":[12,-3,19]}),
                None,
                &flag,
                |_| unreachable!()
            )
            .unwrap(),
        28
    );
    let token = worker.session().to_owned();
    let result = worker
        .call(
            "test-client",
            "identity",
            json!({"owner":"forged"}),
            None,
            &flag,
            |request| {
                assert_eq!(request.session, token);
                assert_eq!(request.method, "storage.get");
                Ok(json!("provider-owned-data"))
            },
        )
        .unwrap();
    assert_eq!(
        result,
        json!({"caller":"test-client","value":"provider-owned-data"})
    );
    let mut progress = false;
    let result = worker.call(
        "test-client",
        "wait-cancel",
        json!({}),
        Some("task-1"),
        &flag,
        |request| {
            assert_eq!(request.params["sequence"], 1);
            progress = true;
            flag.store(true, Ordering::SeqCst);
            Ok(json!(true))
        },
    );
    assert!(progress);
    assert_eq!(result.unwrap_err().code, "CANCELLED");
    drop(worker);
    assert!(stopped(pid));
    let mut worker = Worker::start(&approved, data.path(), &[]).unwrap();
    let pid = worker.pid();
    assert!(worker
        .call(
            "test-client",
            "crash",
            json!({}),
            None,
            &AtomicBool::new(false),
            |_| unreachable!()
        )
        .is_err());
    assert!(stopped(pid));
    drop(worker);
    let worker = Worker::start(&approved, data.path(), &[]).unwrap();
    let pid = worker.pid();
    worker.stop().unwrap();
    assert!(stopped(pid));
}
#[test]
fn forced_cancel_stops_ignored_cancel_and_descendant_process() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), false);
    let approved = approve(root.path());
    let data = tempfile::tempdir().unwrap();
    let mut worker = Worker::start(&approved, data.path(), &[]).unwrap();
    let pid = worker.pid();
    let flag = Arc::new(AtomicBool::new(false));
    let control = flag.clone();
    let pid_file = data.path().join("child.pid");
    let observer_file = pid_file.clone();
    let cancel = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !observer_file.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(observer_file.exists());
        control.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    assert!(
        worker
            .call(
                "test-client",
                "hang-child",
                json!({}),
                Some("task-2"),
                &flag,
                |_| unreachable!()
            )
            .unwrap_err()
            .code
            == "CANCELLED"
    );
    cancel.join().unwrap();
    let child_pid = fs::read_to_string(pid_file)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    drop(worker);
    assert!(stopped(pid));
    assert!(stopped(child_pid));
    assert!(started.elapsed() < Duration::from_secs(15));
}
#[test]
fn bad_protocol_architecture_integrity_and_consent_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    package(root.path(), true);
    let approved = approve(root.path());
    let data = tempfile::tempdir().unwrap();
    assert!(Worker::start(&approved, data.path(), &[]).is_err());
    let root = tempfile::tempdir().unwrap();
    let mut manifest = package(root.path(), false);
    let inspection = inspect_plugin(root.path(), &[], "2.2.0-alpha.1").unwrap();
    let digest = inspection.manifest_sha256();
    assert!(inspection
        .approve(Approval {
            manifest_sha256: digest,
            accept_native_code: false,
            accept_unsigned_local: true,
            accept_permissions: true
        })
        .is_err());
    fs::write(root.path().join("dist/renderer.js"), b"tampered").unwrap();
    assert!(inspect_plugin(root.path(), &[], "2.2.0-alpha.1").is_err());
    package(root.path(), false);
    let exe = root.path().join("bin/backend.exe");
    let mut bytes = fs::read(&exe).unwrap();
    let offset = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
    bytes[offset + 4..offset + 6].copy_from_slice(&0x014cu16.to_le_bytes());
    fs::write(&exe, &bytes).unwrap();
    manifest["files"][0]["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    fs::write(root.path().join("plugin.json"), manifest.to_string()).unwrap();
    assert!(inspect_plugin(root.path(), &[], "2.2.0-alpha.1")
        .err()
        .unwrap()
        .contains("ARCHITECTURE"));
}
