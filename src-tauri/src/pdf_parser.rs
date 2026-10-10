//! PDF operations run exclusively in the document worker process.
pub use cruciblebox_document::pdf_text::coalesce_native_text_fragments;
use cruciblebox_document_worker::{client::Client, protocol::Operation};
use serde_json::{json, Value};
use std::path::Path;
pub const MAX_PDF_PAGES: usize = 2000;
pub const PDF_RENDER_DPI: u16 = 240;
pub fn renderer_status(client: &Client) -> Value {
    match crate::document_worker::run(client, Operation::Status, None) {
        Ok(job) => job.value.clone(),
        Err(error) => json!({"available":false,"error":error}),
    }
}
pub fn render_page_to_png(
    client: &Client,
    path: &str,
    page: u32,
    output: &Path,
    ctx: &crate::task_runtime::Context,
) -> Result<(u32, u32), String> {
    let job = crate::document_worker::run(
        client,
        Operation::Render {
            path: path.into(),
            page,
        },
        Some(ctx),
    )?;
    let source = job.value["path"]
        .as_str()
        .ok_or("DOCUMENT_RENDER_MISSING")?;
    crate::document_worker::publish(&job, source, output, None, None)?;
    Ok((
        job.value["width"].as_u64().ok_or("DOCUMENT_RENDER_WIDTH")? as u32,
        job.value["height"]
            .as_u64()
            .ok_or("DOCUMENT_RENDER_HEIGHT")? as u32,
    ))
}
pub fn split_pdf_file_with_publication(
    client: &Client,
    path: &str,
    output: &Path,
    pages_per_file: usize,
    ctx: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    crate::document_worker::batch(
        client,
        Operation::Split {
            path: path.into(),
            pages_per_file,
            ranges: None,
        },
        output,
        ctx,
    )
}
pub fn split_pdf_file_with_ranges_with_publication(
    client: &Client,
    path: &str,
    output: &Path,
    ranges: &[(usize, usize)],
    ctx: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    crate::document_worker::batch(
        client,
        Operation::Split {
            path: path.into(),
            pages_per_file: 1,
            ranges: Some(ranges.into()),
        },
        output,
        ctx,
    )
}
pub fn merge_pdf_files_with_publication(
    client: &Client,
    paths: &[String],
    output: &Path,
    ctx: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    if output.exists() {
        return Err("DOCUMENT_OUTPUT_EXISTS".into());
    }
    crate::document_worker::single(
        client,
        Operation::Merge {
            paths: paths.into(),
        },
        output,
        ctx.map(|ctx| (ctx, false)),
    )
}
pub fn rotate_pdf_pages_with_publication(
    client: &Client,
    path: &str,
    pages: &[usize],
    degrees: u16,
    output: &Path,
    ctx: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    if output.exists() || Path::new(path) == output {
        return Err("DOCUMENT_OUTPUT_EXISTS".into());
    }
    crate::document_worker::single(
        client,
        Operation::Rotate {
            path: path.into(),
            pages: pages.into(),
            degrees,
        },
        output,
        ctx.map(|ctx| (ctx, false)),
    )
}
pub fn reorder_pdf_pages_with_publication(
    client: &Client,
    path: &str,
    pages: &[usize],
    output: &Path,
    ctx: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    if output.exists() || Path::new(path) == output {
        return Err("DOCUMENT_OUTPUT_EXISTS".into());
    }
    crate::document_worker::single(
        client,
        Operation::Reorder {
            path: path.into(),
            pages: pages.into(),
        },
        output,
        ctx.map(|ctx| (ctx, false)),
    )
}
pub fn extract_pdf_images_with_publication(
    client: &Client,
    path: &str,
    output: &Path,
    ctx: Option<&crate::task_runtime::Context>,
) -> Result<Value, String> {
    crate::document_worker::batch(
        client,
        Operation::ExtractImages { path: path.into() },
        output,
        ctx,
    )
}
