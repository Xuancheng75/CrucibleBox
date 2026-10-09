//! Document conversion runs outside the host; publication remains durable in the task runtime.
use cruciblebox_document_worker::{client::Client, protocol::Operation};
use serde_json::Value;
use std::path::{Path, PathBuf};
pub fn convert_document_with_publication(
    client: &Client,
    document: &Value,
    target: &str,
    output_path: Option<&str>,
    cache_directory: Option<&str>,
    publication: Option<(&crate::task_runtime::Context, bool)>,
) -> Result<Value, String> {
    let target = normalize_target(target)?;
    let source = document["source"]["path"].as_str().unwrap_or("");
    let destination = match output_path.filter(|p| !p.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => output_path_in_directory(
            source,
            target,
            Path::new(source)
                .parent()
                .unwrap_or(Path::new("."))
                .to_str()
                .ok_or("DOCUMENT_OUTPUT_ENCODING")?,
        )?,
    };
    if Path::new(source) == destination
        || Path::new(source)
            .canonicalize()
            .ok()
            .zip(destination.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
    {
        return Err("DOCUMENT_INPUT_OVERWRITE".into());
    }
    let cache_key = crate::document_engine_cache::cache_key(
        document["source"]["hash"].as_str().unwrap_or(""),
        document["source"]["engine"].as_str().unwrap_or("native"),
        document["source"]["engineVersion"].as_str().unwrap_or("1"),
        &serde_json::json!({"target":target}),
    );
    let result = crate::document_worker::single(
        client,
        Operation::Convert {
            document: document.clone(),
            target: target.into(),
        },
        &destination,
        publication,
    )?;
    if let Some(cache) = cache_directory {
        crate::document_engine_cache::write_result(Path::new(cache), &cache_key, &result)?;
    }
    Ok(result)
}
pub fn export_document_bundle_with_publication(
    client: &Client,
    document: &Value,
    output_directory: &Path,
    stem: &str,
    context: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    crate::document_worker::batch(
        client,
        Operation::Export {
            document: document.clone(),
            stem: stem.into(),
        },
        output_directory,
        context,
    )
}
fn normalize_target(target: &str) -> Result<&'static str, String> {
    match target
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "txt" | "text" => Ok("txt"),
        "md" | "markdown" => Ok("md"),
        "html" | "htm" => Ok("html"),
        "json" => Ok("json"),
        "docx" => Ok("docx"),
        "pdf" => Ok("pdf"),
        _ => Err("目标格式支持 TXT/Markdown/HTML/JSON/DOCX/PDF".into()),
    }
}

pub fn output_path_in_directory(
    source_path: &str,
    target: &str,
    output_directory: &str,
) -> Result<PathBuf, String> {
    let target = normalize_target(target)?;
    let source = Path::new(source_path);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let same_format = source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(target));
    let file_name = if same_format {
        format!("{stem}-converted.{target}")
    } else {
        format!("{stem}.{target}")
    };
    Ok(Path::new(output_directory).join(file_name))
}
