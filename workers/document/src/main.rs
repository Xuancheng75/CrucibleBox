#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use cruciblebox_document_native::{
    document_converter as converter, document_parser as parser, pdf_parser as pdf,
};
use cruciblebox_document_worker::protocol::*;
use serde_json::{json, Value};
use std::{io::Write, path::Path};
fn execute(operation: Operation, job: &Path) -> Result<Value, String> {
    let output = job.join("artifacts");
    std::fs::create_dir(&output).map_err(|e| e.to_string())?;
    let file = output.join("result.pdf");
    match operation {
        #[cfg(feature = "acceptance-faults")]
        Operation::Fault { kind } => {
            let status = pdf::renderer_status();
            if status["available"] != true {
                return Err("PDFIUM_REQUIRED_FOR_FAULT_ACCEPTANCE".into());
            }
            match kind.as_str() {
                "native-abort" => std::process::abort(),
                "hang" => {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    Ok(json!({}))
                }
                _ => Err("INVALID_FAULT".into()),
            }
        }
        Operation::Status => Ok(pdf::renderer_status()),
        Operation::Parse { path } => parser::parse_file(&path),
        Operation::Render { path, page } => {
            let file = output.join("page.png");
            let (w, h) = pdf::render_page_to_png(&path, page, &file)?;
            Ok(json!({"path":file,"width":w,"height":h}))
        }
        Operation::Split {
            path,
            pages_per_file,
            ranges,
        } => match ranges {
            Some(ranges) => {
                pdf::split_pdf_file_with_ranges_with_publication(&path, &output, &ranges, None)
            }
            None => pdf::split_pdf_file_with_publication(&path, &output, pages_per_file, None),
        },
        Operation::Merge { paths } => pdf::merge_pdf_files_with_publication(&paths, &file, None),
        Operation::Rotate {
            path,
            pages,
            degrees,
        } => pdf::rotate_pdf_pages_with_publication(&path, &pages, degrees, &file, None),
        Operation::Reorder { path, pages } => {
            pdf::reorder_pdf_pages_with_publication(&path, &pages, &file, None)
        }
        Operation::ExtractImages { path } => {
            pdf::extract_pdf_images_with_publication(&path, &output, None)
        }
        Operation::Convert { document, target } => {
            if !matches!(
                target.as_str(),
                "txt" | "md" | "html" | "json" | "docx" | "pdf"
            ) {
                return Err("UNSUPPORTED_DOCUMENT_TARGET".into());
            }
            let file = output.join(format!("converted.{target}"));
            converter::convert_document_with_cache(&document, &target, file.to_str(), None)
        }
        Operation::Export { document, stem } => {
            converter::export_document_bundle_with_publication(&document, &output, &stem, None)
        }
    }
}
fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("DOCUMENT_WORKER_ARGUMENTS".into());
    }
    let job = Path::new(&args[0]);
    let meta = std::fs::symlink_metadata(job).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("INVALID_DOCUMENT_JOB".into());
    }
    let request: Request =
        serde_json::from_slice(&read_frame(std::io::stdin())?).map_err(|e| e.to_string())?;
    if request.wire_version != WIRE_VERSION
        || request.nonce != args[1]
        || request.request_id != args[2]
    {
        return Err("DOCUMENT_SESSION_DENIED".into());
    }
    let operation: Operation = read_reference(&job.join("input.json"), &request.input)?;
    let result = execute(operation, job).and_then(|value| {
        let artifacts = collect_artifacts(&job.join("artifacts"))?;
        write_reference(&job.join("result.json"), &Completed { value, artifacts })
    });
    let (result, error) = match result {
        Ok(v) => (Some(v), None),
        Err(e) => (None, Some(e.chars().take(2048).collect())),
    };
    let response = Response {
        wire_version: WIRE_VERSION,
        nonce: request.nonce,
        request_id: request.request_id,
        result,
        error,
    };
    let bytes = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    if bytes.len() + 1 > MAX_FRAME {
        return Err("DOCUMENT_FRAME_BUDGET".into());
    }
    let mut output = std::io::stdout().lock();
    output
        .write_all(&bytes)
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("[document-worker] {e}");
        std::process::exit(1)
    }
}
