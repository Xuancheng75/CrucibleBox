//! Host adapter: the child owns native libraries; only validated artifacts are published here.
use cruciblebox_document_worker::{
    client::{Client, JobResult},
    protocol::Operation,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
pub fn run(
    client: &Client,
    operation: Operation,
    context: Option<&crate::task_runtime::Context>,
) -> Result<JobResult, String> {
    client.run(&operation, &|| {
        context.is_some_and(|ctx| ctx.is_cancelled())
    })
}
pub fn publish(
    job: &JobResult,
    source: &str,
    target: &Path,
    context: Option<(&crate::task_runtime::Context, bool)>,
    reference: Option<&Path>,
) -> Result<PathBuf, String> {
    let source = job.artifact(source)?;
    if let Some((ctx, _)) = context {
        ctx.check_cancelled()?;
    }
    let transaction = crate::output_transaction::OutputTransaction::new(target, false)?;
    std::fs::copy(&source, transaction.stage_path()).map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(transaction.stage_path())
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    let expected = cruciblebox_document_worker::protocol::artifact_reference(&source)?;
    let validate = |stage: &Path| {
        let actual = cruciblebox_document_worker::protocol::artifact_reference(stage)?;
        if actual.bytes != expected.bytes || actual.sha256 != expected.sha256 {
            return Err("DOCUMENT_ARTIFACT_CHANGED".into());
        }
        Ok(())
    };
    match (context, reference) {
        (Some((ctx, _)), Some(reference)) => {
            transaction.publish_durable_with_reference(ctx, reference, validate)
        }
        (Some((ctx, terminal)), None) => transaction.publish_durable(ctx, terminal, validate),
        (None, _) => transaction.publish(validate),
    }
}
pub fn batch(
    client: &Client,
    operation: Operation,
    directory: &Path,
    context: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    let job = run(client, operation, context)?;
    let mut value = job.value.clone();
    let files = value["files"]
        .as_array_mut()
        .ok_or("DOCUMENT_FILES_MISSING")?;
    for file in files {
        let source = file["path"].as_str().ok_or("DOCUMENT_OUTPUT_MISSING")?;
        let artifact = job.artifact(source)?;
        let target = directory.join(artifact.file_name().ok_or("DOCUMENT_OUTPUT_NAME")?);
        file["path"] = json!(publish(
            &job,
            source,
            &target,
            context.map(|ctx| (ctx, false)),
            Some(directory)
        )?);
    }
    if value.get("outputDirectory").is_some() {
        value["outputDirectory"] = json!(directory);
    }
    if value.get("directory").is_some() {
        value["directory"] = json!(directory);
    }
    Ok(value)
}
pub fn single(
    client: &Client,
    operation: Operation,
    target: &Path,
    context: Option<(&crate::task_runtime::Context, bool)>,
) -> Result<Value, String> {
    let job = run(client, operation, context.map(|v| v.0))?;
    let source = job.value["outputPath"]
        .as_str()
        .ok_or("DOCUMENT_OUTPUT_MISSING")?;
    let output = publish(&job, source, target, context, None)?;
    let mut value = job.value.clone();
    value["outputPath"] = json!(output);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires DOCUMENT_WORKER_ACCEPTANCE_EXE and a separately built document worker"]
    fn native_document_artifacts_publish_durably_and_survive_restart() {
        let root = tempfile::tempdir().unwrap();
        let client = Client::new(
            std::env::var_os("DOCUMENT_WORKER_ACCEPTANCE_EXE")
                .expect("worker acceptance executable")
                .into(),
            None,
            root.path().join("jobs"),
            std::time::Duration::from_secs(15),
        );
        let source = root.path().join("input.pdf");
        let pdf = b"%PDF-1.4\n1 0 obj\n<</Type /Page /MediaBox [0 0 200 100] /Contents 2 0 R>>\nendobj\n2 0 obj\n<</Length 23>>\nstream\nBT (Hi) Tj ET\nendstream\nendobj\n%%EOF";
        std::fs::write(&source, pdf).unwrap();
        let journal = root.path().join("tasks.sqlite");
        let output = root.path().join("output");
        let runtime = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        let result = runtime
            .run_sync(
                "document-engine",
                "parse",
                Some("native-publication"),
                |ctx| {
                    let parsed = run(
                        &client,
                        Operation::Parse {
                            path: source.to_string_lossy().into(),
                        },
                        Some(ctx),
                    )?;
                    batch(
                        &client,
                        Operation::Export {
                            document: parsed.value["document"].clone(),
                            stem: "document".into(),
                        },
                        &output,
                        Some(ctx),
                    )
                },
            )
            .unwrap();
        assert_eq!(result["files"].as_array().unwrap().len(), 3);
        for item in result["files"].as_array().unwrap() {
            assert!(
                std::fs::metadata(item["path"].as_str().unwrap())
                    .unwrap()
                    .len()
                    > 0
            );
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("jobs")).unwrap().count(),
            0
        );
        let snapshot = runtime
            .get("document-engine", "native-publication")
            .unwrap();
        assert_eq!(snapshot["status"], "succeeded");
        assert_eq!(
            snapshot["resultRefs"],
            json!([output.canonicalize().unwrap()])
        );
        drop(runtime);
        let recovered = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        assert_eq!(
            recovered
                .get("document-engine", "native-publication")
                .unwrap()["resultRefs"],
            snapshot["resultRefs"]
        );
        let cancelled = root.path().join("cancelled");
        assert!(recovered
            .run_sync(
                "document-engine",
                "parse",
                Some("cancel-before-publish"),
                |ctx| {
                    let parsed = run(
                        &client,
                        Operation::Parse {
                            path: source.to_string_lossy().into(),
                        },
                        Some(ctx),
                    )?;
                    assert!(recovered.cancel("document-engine", "cancel-before-publish"));
                    batch(
                        &client,
                        Operation::Export {
                            document: parsed.value["document"].clone(),
                            stem: "cancelled".into(),
                        },
                        &cancelled,
                        Some(ctx),
                    )
                }
            )
            .is_err());
        assert!(!cancelled.exists());
        assert_eq!(
            recovered
                .get("document-engine", "cancel-before-publish")
                .unwrap()["status"],
            "cancelled"
        );

        let corrupt = root.path().join("corrupt.pdf");
        std::fs::write(&corrupt, b"not a pdf").unwrap();
        assert!(recovered
            .run_sync("document-engine", "parse", Some("native-failure"), |ctx| {
                run(
                    &client,
                    Operation::Parse {
                        path: corrupt.to_string_lossy().into(),
                    },
                    Some(ctx),
                )
                .map(|job| job.value.clone())
            })
            .is_err());
        let failed = recovered.get("document-engine", "native-failure").unwrap();
        assert_eq!(failed["status"], "failed");
        assert!(!failed["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .is_empty());
        drop(recovered);

        let restarted = crate::task_runtime::TaskRuntime::open(&journal).unwrap();
        assert_eq!(
            restarted
                .get("document-engine", "native-publication")
                .unwrap()["resultRefs"],
            snapshot["resultRefs"]
        );
        assert_eq!(
            restarted
                .get("document-engine", "cancel-before-publish")
                .unwrap()["status"],
            "cancelled"
        );
        let recovered_failure = restarted.get("document-engine", "native-failure").unwrap();
        assert_eq!(recovered_failure["status"], "failed");
        assert_eq!(recovered_failure["error"], failed["error"]);
        assert_eq!(recovered_failure["resultRefs"], json!([]));
    }
}
