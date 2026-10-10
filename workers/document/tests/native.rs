use cruciblebox_document_worker::{client::Client, protocol::*};

use serde_json::json;
#[cfg(feature = "acceptance-faults")]
use std::time::Instant;
use std::{fs, time::Duration};
fn pdfium() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/resources/pdfium.dll")
}
#[test]
fn real_document_process_returns_ir_by_digest_and_keeps_artifacts_leased() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("中文.txt");
    fs::write(
        &input,
        "# 文档

独立 worker 内容",
    )
    .unwrap();
    let client = Client::new(
        env!("CARGO_BIN_EXE_document-worker").into(),
        Some(pdfium()),
        root.path().join("jobs"),
        Duration::from_secs(10),
    );
    let parsed = client
        .run(
            &Operation::Parse {
                path: input.to_string_lossy().into(),
            },
            &|| false,
        )
        .unwrap();
    assert!(parsed.value["document"]["pages"].is_array());
    let converted = client
        .run(
            &Operation::Convert {
                document: parsed.value["document"].clone(),
                target: "docx".into(),
            },
            &|| false,
        )
        .unwrap();
    let artifact = converted
        .artifact(converted.value["outputPath"].as_str().unwrap())
        .unwrap();
    assert!(fs::metadata(&artifact).unwrap().len() > 100);
    assert!(converted.artifact(input.to_str().unwrap()).is_err());
    fs::write(&artifact, b"tampered output").unwrap();
    assert!(converted.artifact(artifact.to_str().unwrap()).is_err());
    let job = converted.root().to_path_buf();
    drop(converted);
    assert!(!job.exists());
}
#[test]
fn reference_tampering_and_oversized_frames_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("input.json");
    let r = write_reference(&file, &json!({"text":"原值"})).unwrap();
    fs::write(&file, "{}").unwrap();
    assert!(read_reference::<serde_json::Value>(&file, &r).is_err());
    assert!(read_frame(vec![b'x'; MAX_FRAME + 1].as_slice()).is_err());
    assert!(read_frame(b"{}".as_slice()).is_err());
    assert!(serde_json::from_value::<Operation>(
        json!({"operation":"parse","path":"x","extra":true})
    )
    .is_err());
}
#[cfg(feature = "acceptance-faults")]
#[test]
fn pdfium_native_process_abort_timeout_and_cancel_leave_parent_and_next_request_usable() {
    let root = tempfile::tempdir().unwrap();
    let jobs = root.path().join("jobs");
    let client = Client::new(
        env!("CARGO_BIN_EXE_document-worker").into(),
        Some(pdfium()),
        jobs.clone(),
        Duration::from_secs(5),
    );
    let status = client.run(&Operation::Status, &|| false).unwrap();
    assert_eq!(status.value["available"], true, "{}", status.value);
    drop(status);
    let error = client
        .run(
            &Operation::Fault {
                kind: "native-abort".into(),
            },
            &|| false,
        )
        .err()
        .expect("native abort must fail");
    assert!(error.contains("DOCUMENT_WORKER_EXITED"), "{error}");
    let short = Client::new(
        env!("CARGO_BIN_EXE_document-worker").into(),
        Some(pdfium()),
        jobs.clone(),
        Duration::from_millis(600),
    );
    let error = short
        .run(
            &Operation::Fault {
                kind: "hang".into(),
            },
            &|| false,
        )
        .err()
        .expect("hung process must time out");
    assert_eq!(error, "DOCUMENT_TIMEOUT");
    let started = Instant::now();
    let error = client
        .run(
            &Operation::Fault {
                kind: "hang".into(),
            },
            &|| started.elapsed() > Duration::from_millis(500),
        )
        .err()
        .expect("cancel must terminate");
    assert_eq!(error, "DOCUMENT_CANCELLED");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(fs::read_dir(&jobs).unwrap().count(), 0);
    let input = root.path().join("recovered.txt");
    fs::write(&input, "可恢复").unwrap();
    let recovered = client
        .run(
            &Operation::Parse {
                path: input.to_string_lossy().into(),
            },
            &|| false,
        )
        .unwrap();
    assert!(recovered.value["document"].is_object());
}
