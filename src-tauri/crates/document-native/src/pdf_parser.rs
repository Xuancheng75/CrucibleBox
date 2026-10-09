//! PDF parser and PDFium renderer used by Document Engine.
//!
//! Text-layer extraction stays dependency-light and deterministic. Scanned
//! pages are rendered by the packaged PDFium runtime and then handed to the
//! Rust OCR worker by the trusted service.

use cruciblebox_document::pdf_text::*;
use flate2::read::ZlibDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_PDF_PAGES: usize = 2000;
const MAX_OBJECTS: usize = 100_000;
/// OCR rendering target.  A 240 DPI page keeps small mathematical glyphs
/// legible while the caps prevent a very large page from exhausting memory.
pub const PDF_RENDER_DPI: f32 = 240.0;
const PDF_RENDER_MIN_WIDTH: f32 = 1400.0;
const PDF_RENDER_MAX_WIDTH: f32 = 2800.0;
const PDF_RENDER_MIN_HEIGHT: f32 = 1800.0;
const PDF_RENDER_MAX_HEIGHT: i32 = 3800;

#[derive(Debug)]
struct PdfObject {
    id: u32,
    dictionary: Vec<u8>,
    stream: Option<Vec<u8>>,
}

/// Parse a PDF into the unified Document JSON shape.
///
/// The return value is successful for both text and scanned PDFs.  Scanned
/// pages are represented as empty pages and listed in ocrPageNumbers; the
/// trusted service fills those pages after PDFium rendering and OCR.
pub fn parse_file(path: &str) -> Result<Value, String> {
    let file_path = Path::new(path);
    let metadata =
        std::fs::metadata(file_path).map_err(|error| format!("无法访问 PDF: {error}"))?;
    if metadata.len() > MAX_PDF_BYTES {
        return Err(format!(
            "PDF exceeds {} MiB limit",
            MAX_PDF_BYTES / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(file_path).map_err(|error| format!("读取 PDF 失败: {error}"))?;
    parse_bytes(path, &bytes)
}

/// Split a PDF into real, independently readable PDF files. This is kept
/// separate from the text chunker: callers asking for PDF splitting must get
/// PDF artifacts, not a JSON RAG manifest.
#[allow(dead_code)]
pub fn split_pdf_file(
    path: &str,
    output_directory: &Path,
    pages_per_file: usize,
) -> Result<Value, String> {
    split_pdf_file_with_publication(path, output_directory, pages_per_file, None)
}

pub fn split_pdf_file_with_publication(
    path: &str,
    output_directory: &Path,
    pages_per_file: usize,
    context: Option<&cruciblebox_task_runtime::Context>,
) -> Result<Value, String> {
    if !(1..=MAX_PDF_PAGES).contains(&pages_per_file) {
        return Err(format!(
            "每个 PDF 文件的页数必须在 1..={MAX_PDF_PAGES} 之间"
        ));
    }
    let pdfium = bind_pdfium()?;
    let source = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("加载 PDF 失败: {error}"))?;
    let page_count = source.pages().len() as usize;
    if page_count == 0 {
        return Err("PDF 不包含可拆分的页面".into());
    }
    if page_count > MAX_PDF_PAGES {
        return Err(format!("PDF 页数超过 {MAX_PDF_PAGES} 页限制"));
    }
    std::fs::create_dir_all(output_directory)
        .map_err(|error| format!("创建 PDF 拆分输出目录失败: {error}"))?;
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    let stem = if stem.trim().is_empty() {
        "document"
    } else {
        stem.trim()
    };
    let mut files = Vec::new();
    for start in (0..page_count).step_by(pages_per_file) {
        let end = (start + pages_per_file).min(page_count) - 1;
        let part_number = files.len() + 1;
        let destination =
            output_directory.join(format!("{stem}_{:03}-{:03}页.pdf", start + 1, end + 1));
        let output = crate::output_transaction::OutputTransaction::new(&destination, false)?;
        let mut part = pdfium
            .create_new_pdf()
            .map_err(|error| format!("创建拆分 PDF 失败: {error}"))?;
        part.pages_mut()
            .copy_page_range_from_document(&source, (start as i32)..=(end as i32), 0)
            .map_err(|error| format!("复制 PDF 页面 {}-{} 失败: {error}", start + 1, end + 1))?;
        part.save_to_file(output.stage_path())
            .map_err(|error| format!("写入拆分 PDF 失败: {error}"))?;
        drop(part);
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
        }
        let validate = |stage: &Path| {
            let verified = pdfium
                .load_pdf_from_file(stage, None)
                .map_err(|error| format!("验证拆分 PDF 失败: {error}"))?;
            if verified.pages().len() as usize != end - start + 1 {
                return Err("拆分 PDF 页数不正确".into());
            }
            Ok(())
        };
        let committed = match context {
            Some(ctx) => output.publish_durable_with_reference(ctx, output_directory, validate),
            None => output.publish(validate),
        }?;
        files.push(json!({
            "index": part_number,
            "path": committed.to_string_lossy(),
            "startPage": start + 1,
            "endPage": end + 1,
            "pageCount": end - start + 1
        }));
    }
    Ok(json!({
        "sourcePath": path,
        "outputDirectory": output_directory.to_string_lossy(),
        "pageCount": page_count,
        "pagesPerFile": pages_per_file,
        "fileCount": files.len(),
        "files": files
    }))
}

/// Split a PDF using explicit inclusive page ranges.  This is the physical
/// PDF splitter counterpart to text Chunk splitting: every returned artifact
/// is a standalone, readable PDF file.
#[allow(dead_code)]
pub fn split_pdf_file_with_ranges(
    path: &str,
    output_directory: &Path,
    ranges: &[(usize, usize)],
) -> Result<Value, String> {
    split_pdf_file_with_ranges_with_publication(path, output_directory, ranges, None)
}

pub fn split_pdf_file_with_ranges_with_publication(
    path: &str,
    output_directory: &Path,
    ranges: &[(usize, usize)],
    context: Option<&cruciblebox_task_runtime::Context>,
) -> Result<Value, String> {
    if ranges.is_empty() {
        return Err("至少需要一个 PDF 页码范围".into());
    }
    let pdfium = bind_pdfium()?;
    let source = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("加载 PDF 失败: {error}"))?;
    let page_count = source.pages().len() as usize;
    if page_count == 0 {
        return Err("PDF 不包含可拆分的页面".into());
    }
    if page_count > MAX_PDF_PAGES {
        return Err(format!("PDF 页数超过 {MAX_PDF_PAGES} 页限制"));
    }
    for (start, end) in ranges {
        if *start == 0 || *end < *start || *end > page_count {
            return Err(format!(
                "页码范围无效: {start}-{end}（PDF 共 {page_count} 页）"
            ));
        }
    }
    std::fs::create_dir_all(output_directory)
        .map_err(|error| format!("创建 PDF 拆分输出目录失败: {error}"))?;
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    let stem = if stem.trim().is_empty() {
        "document"
    } else {
        stem.trim()
    };
    let mut files = Vec::with_capacity(ranges.len());
    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let destination = output_directory.join(format!("{stem}_{:03}-{:03}页.pdf", start, end));
        let output = crate::output_transaction::OutputTransaction::new(&destination, false)?;
        let mut part = pdfium
            .create_new_pdf()
            .map_err(|error| format!("创建拆分 PDF 失败: {error}"))?;
        part.pages_mut()
            .copy_page_range_from_document(&source, ((start - 1) as i32)..=(end - 1) as i32, 0)
            .map_err(|error| format!("复制 PDF 页面 {start}-{end} 失败: {error}"))?;
        part.save_to_file(output.stage_path())
            .map_err(|error| format!("写入拆分 PDF 失败: {error}"))?;
        drop(part);
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
        }
        let validate = |stage: &Path| {
            let verified = pdfium
                .load_pdf_from_file(stage, None)
                .map_err(|error| format!("验证拆分 PDF 失败: {error}"))?;
            if verified.pages().len() as usize != end - start + 1 {
                return Err("拆分 PDF 页数不正确".into());
            }
            Ok(())
        };
        let committed = match context {
            Some(ctx) => output.publish_durable_with_reference(ctx, output_directory, validate),
            None => output.publish(validate),
        }?;
        files.push(json!({
            "index": index + 1,
            "path": committed.to_string_lossy(),
            "startPage": start,
            "endPage": end,
            "pageCount": end - start + 1
        }));
    }
    Ok(json!({
        "sourcePath": path,
        "outputDirectory": output_directory.to_string_lossy(),
        "pageCount": page_count,
        "fileCount": files.len(),
        "files": files
    }))
}

/// Combine pages from multiple PDFs into a new, independently readable file.
/// The caller chooses the destination; source files are never modified.
#[cfg(test)]
pub fn merge_pdf_files(paths: &[String], output: &Path) -> Result<Value, String> {
    merge_pdf_files_with_publication(paths, output, None)
}
pub fn merge_pdf_files_with_publication(
    paths: &[String],
    output: &Path,
    context: Option<&cruciblebox_task_runtime::Context>,
) -> Result<Value, String> {
    if paths.len() < 2 {
        return Err("PDF 合并至少需要两个源文件".into());
    }
    if output.exists() {
        return Err("输出文件已存在，请选择新文件名".into());
    }
    let pdfium = bind_pdfium()?;
    let mut merged = pdfium
        .create_new_pdf()
        .map_err(|error| format!("创建合并 PDF 失败: {error}"))?;
    let mut total_pages = 0usize;
    for path in paths {
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
        }
        if Path::new(path) == output {
            return Err("输出路径不能与源文件相同".into());
        }
        let source = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|error| format!("加载 PDF {} 失败: {error}", path))?;
        let count = source.pages().len() as usize;
        if count == 0 || total_pages + count > MAX_PDF_PAGES {
            return Err(format!("PDF 合并页数必须在 1..={MAX_PDF_PAGES} 之间"));
        }
        merged
            .pages_mut()
            .copy_page_range_from_document(&source, 0..=(count - 1) as i32, total_pages as i32)
            .map_err(|error| format!("合并 PDF {} 失败: {error}", path))?;
        total_pages += count;
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建输出目录失败: {error}"))?;
    }
    if let Some(ctx) = context {
        ctx.check_cancelled()?;
    }
    let transaction = crate::output_transaction::OutputTransaction::new(output, false)?;
    merged
        .save_to_file(transaction.stage_path())
        .map_err(|error| format!("保存合并 PDF 失败: {error}"))?;
    drop(merged);
    if let Some(ctx) = context {
        ctx.check_cancelled()?;
    }
    let validate = |stage: &Path| {
        let verified = pdfium
            .load_pdf_from_file(stage, None)
            .map_err(|error| error.to_string())?;
        if verified.pages().len() as usize != total_pages {
            return Err("生成 PDF 页数不正确".into());
        }
        Ok(())
    };
    let published = match context {
        Some(ctx) => transaction.publish_durable(ctx, true, validate),
        None => transaction.publish(validate),
    }?;
    Ok(
        json!({ "outputPath": published.to_string_lossy(), "pageCount": total_pages, "fileCount": paths.len() }),
    )
}

/// Copy pages into a caller-specified order. Page numbers are 1-based and may
/// be repeated or omitted, enabling both reorder and extraction workflows.
#[cfg(test)]
pub fn reorder_pdf_pages(path: &str, pages: &[usize], output: &Path) -> Result<Value, String> {
    reorder_pdf_pages_with_publication(path, pages, output, None)
}
pub fn reorder_pdf_pages_with_publication(
    path: &str,
    pages: &[usize],
    output: &Path,
    context: Option<&cruciblebox_task_runtime::Context>,
) -> Result<Value, String> {
    if pages.is_empty() || pages.len() > MAX_PDF_PAGES {
        return Err("请提供有效的 PDF 页码顺序".into());
    }
    if output.exists() || Path::new(path) == output {
        return Err("输出文件已存在，请选择新文件名".into());
    }
    let pdfium = bind_pdfium()?;
    let source = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("加载 PDF 失败: {error}"))?;
    let source_pages = source.pages().len() as usize;
    let mut reordered = pdfium
        .create_new_pdf()
        .map_err(|error| format!("创建重排 PDF 失败: {error}"))?;
    for (index, page) in pages.iter().copied().enumerate() {
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
        }
        if page == 0 || page > source_pages {
            return Err(format!("第 {page} 页不存在，源文件共 {source_pages} 页"));
        }
        reordered
            .pages_mut()
            .copy_page_range_from_document(
                &source,
                (page - 1) as i32..=(page - 1) as i32,
                index as i32,
            )
            .map_err(|error| format!("复制第 {page} 页失败: {error}"))?;
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建输出目录失败: {error}"))?;
    }
    if let Some(ctx) = context {
        ctx.check_cancelled()?;
    }
    let transaction = crate::output_transaction::OutputTransaction::new(output, false)?;
    reordered
        .save_to_file(transaction.stage_path())
        .map_err(|error| format!("保存重排 PDF 失败: {error}"))?;
    drop(reordered);
    if let Some(ctx) = context {
        ctx.check_cancelled()?;
    }
    let validate = |stage: &Path| {
        let verified = pdfium
            .load_pdf_from_file(stage, None)
            .map_err(|error| error.to_string())?;
        if verified.pages().len() as usize != pages.len() {
            return Err("生成 PDF 页数不正确".into());
        }
        Ok(())
    };
    let published = match context {
        Some(ctx) => transaction.publish_durable(ctx, true, validate),
        None => transaction.publish(validate),
    }?;
    Ok(
        json!({ "sourcePath": path, "outputPath": published.to_string_lossy(), "pageCount": pages.len(), "pages": pages }),
    )
}

#[cfg(test)]
pub fn rotate_pdf_pages(
    path: &str,
    pages: &[usize],
    degrees: u16,
    output: &Path,
) -> Result<Value, String> {
    rotate_pdf_pages_with_publication(path, pages, degrees, output, None)
}
pub fn rotate_pdf_pages_with_publication(
    path: &str,
    pages: &[usize],
    degrees: u16,
    output: &Path,
    context: Option<&cruciblebox_task_runtime::Context>,
) -> Result<Value, String> {
    use pdfium_bundled::pdfium_render::prelude::PdfPageRenderRotation;
    let rotation_steps = match degrees {
        90 => 1,
        180 => 2,
        270 => 3,
        _ => return Err("旋转角度必须是 90、180 或 270 度".into()),
    };
    if output.exists() || Path::new(path) == output {
        return Err("输出文件已存在，请选择新文件名".into());
    }
    let pdfium = bind_pdfium()?;
    let source = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("加载 PDF 失败: {error}"))?;
    let page_count = source.pages().len() as usize;
    if page_count == 0 || page_count > MAX_PDF_PAGES {
        return Err("PDF 页数无效".into());
    }
    if pages.iter().any(|page| *page == 0 || *page > page_count) {
        return Err(format!("旋转页码超出范围，源文件共 {page_count} 页"));
    }
    let mut rotated = pdfium
        .create_new_pdf()
        .map_err(|error| format!("创建 PDF 失败: {error}"))?;
    rotated
        .pages_mut()
        .copy_page_range_from_document(&source, 0..=(page_count - 1) as i32, 0)
        .map_err(|error| format!("复制 PDF 页面失败: {error}"))?;
    let selected = if pages.is_empty() {
        (1..=page_count).collect::<Vec<_>>()
    } else {
        pages.to_vec()
    };
    for page_number in &selected {
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
        }
        let mut page = rotated
            .pages()
            .get((*page_number - 1) as i32)
            .map_err(|error| format!("读取第 {page_number} 页失败: {error}"))?;
        let current = match page.rotation().map_err(|error| error.to_string())? {
            PdfPageRenderRotation::None => 0,
            PdfPageRenderRotation::Degrees90 => 1,
            PdfPageRenderRotation::Degrees180 => 2,
            PdfPageRenderRotation::Degrees270 => 3,
        };
        let next = match (current + rotation_steps) % 4 {
            1 => PdfPageRenderRotation::Degrees90,
            2 => PdfPageRenderRotation::Degrees180,
            3 => PdfPageRenderRotation::Degrees270,
            _ => PdfPageRenderRotation::None,
        };
        page.set_rotation(next);
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建输出目录失败: {error}"))?;
    }
    if let Some(ctx) = context {
        ctx.check_cancelled()?;
    }
    let transaction = crate::output_transaction::OutputTransaction::new(output, false)?;
    rotated
        .save_to_file(transaction.stage_path())
        .map_err(|error| format!("保存旋转 PDF 失败: {error}"))?;
    drop(rotated);
    if let Some(ctx) = context {
        ctx.check_cancelled()?;
    }
    let validate = |stage: &Path| {
        let verified = pdfium
            .load_pdf_from_file(stage, None)
            .map_err(|error| error.to_string())?;
        if verified.pages().len() as usize != page_count {
            return Err("生成 PDF 页数不正确".into());
        }
        Ok(())
    };
    let published = match context {
        Some(ctx) => transaction.publish_durable(ctx, true, validate),
        None => transaction.publish(validate),
    }?;
    Ok(
        json!({ "sourcePath": path, "outputPath": published.to_string_lossy(), "pageCount": page_count, "rotatedPages": selected, "degrees": degrees }),
    )
}

#[cfg(test)]
pub fn extract_pdf_images(path: &str, output_directory: &Path) -> Result<Value, String> {
    extract_pdf_images_with_publication(path, output_directory, None)
}
pub fn extract_pdf_images_with_publication(
    path: &str,
    output_directory: &Path,
    context: Option<&cruciblebox_task_runtime::Context>,
) -> Result<Value, String> {
    use pdfium_bundled::pdfium_render::prelude::PdfPageObjectsCommon;
    let pdfium = bind_pdfium()?;
    let source = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("加载 PDF 失败: {error}"))?;
    let page_count = source.pages().len() as usize;
    std::fs::create_dir_all(output_directory)
        .map_err(|error| format!("创建图片输出目录失败: {error}"))?;
    let mut files = Vec::new();
    for page_index in 0..page_count {
        if let Some(ctx) = context {
            ctx.check_cancelled()?;
        }
        let page = source
            .pages()
            .get(page_index as i32)
            .map_err(|error| format!("读取 PDF 第 {} 页失败: {error}", page_index + 1))?;
        let mut image_index = 0usize;
        for object in page.objects().iter() {
            let Some(image) = object.as_image_object() else {
                continue;
            };
            image_index += 1;
            let output = output_directory.join(format!(
                "page_{:03}_image_{image_index:03}.png",
                page_index + 1
            ));
            if output.exists() {
                return Err(format!("输出文件已存在：{}", output.display()));
            }
            let bitmap = image
                .get_raw_image()
                .map_err(|error| format!("提取图片失败: {error}"))?;
            let transaction = crate::output_transaction::OutputTransaction::new(&output, false)?;
            bitmap
                .save_with_format(transaction.stage_path(), image::ImageFormat::Png)
                .map_err(|error| format!("保存图片失败: {error}"))?;
            if let Some(ctx) = context {
                ctx.check_cancelled()?;
            }
            let validate = |stage: &Path| {
                image::open(stage)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            };
            let output = match context {
                Some(ctx) => {
                    transaction.publish_durable_with_reference(ctx, output_directory, validate)
                }
                None => transaction.publish(validate),
            }?;
            files.push(json!({ "page": page_index + 1, "path": output.to_string_lossy() }));
        }
    }
    Ok(
        json!({ "sourcePath": path, "outputDirectory": output_directory.to_string_lossy(), "pageCount": page_count, "imageCount": files.len(), "files": files }),
    )
}

/// Render one PDF page to a PNG for the OCR worker.
///
/// The application ships `pdfium.dll` as a Tauri resource. Development
/// builds may override it with `PDFIUM_LIB_PATH`; when neither is present we
/// fall back to pdfium-bundled's cache/download path so scan-PDF OCR remains
/// testable without a system PDF package.
pub fn render_page_to_png(
    path: &str,
    page_number: u32,
    output: &Path,
) -> Result<(u32, u32), String> {
    if page_number == 0 {
        return Err("PDF 页码必须从 1 开始".into());
    }
    let pdfium = bind_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("加载 PDF 失败: {error}"))?;
    let page = document
        .pages()
        .get((page_number - 1) as i32)
        .map_err(|error| format!("读取 PDF 第 {page_number} 页失败: {error}"))?;
    let target_width = (page.width().value * PDF_RENDER_DPI / 72.0)
        .clamp(PDF_RENDER_MIN_WIDTH, PDF_RENDER_MAX_WIDTH)
        .round() as i32;
    let target_height = (page.height().value * PDF_RENDER_DPI / 72.0)
        .clamp(PDF_RENDER_MIN_HEIGHT, PDF_RENDER_MAX_HEIGHT as f32)
        .round() as i32;
    let config = pdfium_bundled::pdfium_render::prelude::PdfRenderConfig::new()
        .set_target_width(target_width)
        .set_maximum_height(target_height);
    let image = page
        .render_with_config(&config)
        .map_err(|error| format!("渲染 PDF 第 {page_number} 页失败: {error}"))?
        .as_image()
        .map_err(|error| format!("读取 PDF 渲染位图失败: {error}"))?;
    let rgb = image.into_rgb8();
    let dimensions = (rgb.width(), rgb.height());
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建 PDF 渲染目录失败: {error}"))?;
    }
    rgb.save_with_format(output, image::ImageFormat::Png)
        .map_err(|error| format!("保存 PDF 渲染页失败: {error}"))?;
    Ok(dimensions)
}

struct PdfiumRuntime {
    pdfium: pdfium_bundled::pdfium_render::prelude::Pdfium,
    path: Option<PathBuf>,
}

static PDFIUM_RUNTIME: OnceLock<Result<PdfiumRuntime, String>> = OnceLock::new();

fn pdfium_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();
    if let Some(path) = std::env::var_os("PDFIUM_LIB_PATH") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("pdfium.dll"));
            candidates.push(parent.join("resources").join("pdfium.dll"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri/resources/pdfium.dll"));
        candidates.push(cwd.join("resources/pdfium.dll"));
        candidates.push(cwd.join("pdfium.dll"));
    }
    candidates
}

fn bind_pdfium() -> Result<&'static pdfium_bundled::pdfium_render::prelude::Pdfium, String> {
    let runtime = PDFIUM_RUNTIME.get_or_init(initialize_pdfium);
    match runtime {
        Ok(runtime) => Ok(&runtime.pdfium),
        Err(error) => Err(error.clone()),
    }
}

fn initialize_pdfium() -> Result<PdfiumRuntime, String> {
    let mut failures = Vec::new();
    for candidate in pdfium_candidates() {
        if candidate.is_file() {
            match pdfium_bundled::bind_pdfium_from_path(&candidate) {
                Ok(pdfium) => {
                    return Ok(PdfiumRuntime {
                        pdfium,
                        path: candidate.canonicalize().ok().or(Some(candidate)),
                    });
                }
                Err(error) => failures.push(format!("{}: {error}", candidate.display())),
            }
        }
    }
    match pdfium_bundled::bind_pdfium_silent() {
        Ok(pdfium) => Ok(PdfiumRuntime {
            pdfium,
            path: pdfium_bundled::cached_pdfium_path(),
        }),
        Err(error) => {
            let detail = if failures.is_empty() {
                error.to_string()
            } else {
                format!("{}；缓存绑定失败: {error}", failures.join("；"))
            };
            if detail.contains("PdfiumLibraryBindingsAlreadyInitialized") {
                Err("PDFium 已在进程中初始化，但当前服务未能复用该实例".into())
            } else {
                Err(format!(
                    "PDFium 初始化失败（请随包提供 pdfium.dll）: {detail}"
                ))
            }
        }
    }
}

/// Report whether the packaged/system PDFium runtime can be located without
/// downloading anything. Binding is deferred until a scan page is requested.
pub fn renderer_status() -> Value {
    let runtime = PDFIUM_RUNTIME.get();
    let initialized = matches!(runtime, Some(Ok(_)));
    let binding_error = runtime.and_then(|result| result.as_ref().err()).cloned();
    let bound_path = runtime.and_then(|result| {
        result
            .as_ref()
            .ok()
            .and_then(|runtime| runtime.path.clone())
    });
    let path = bound_path
        .map(|value| value.to_string_lossy().into_owned())
        .or_else(|| {
            pdfium_candidates()
                .into_iter()
                .find(|candidate| candidate.is_file())
                .map(|value| value.to_string_lossy().into_owned())
                .or_else(|| {
                    pdfium_bundled::cached_pdfium_path()
                        .map(|value| value.to_string_lossy().into_owned())
                })
        });
    json!({
        "available": binding_error.is_none() && (initialized || path.is_some()),
        "initialized": initialized,
        "path": path,
        "version": pdfium_bundled::PDFIUM_VERSION,
        "runtimeDownload": true,
        "error": binding_error
    })
}

fn parse_bytes(path: &str, bytes: &[u8]) -> Result<Value, String> {
    if !bytes.starts_with(b"%PDF") {
        return Err("输入文件不是有效 PDF（缺少 %PDF 头）".into());
    }

    let source_hash = {
        let mut digest = Sha256::new();
        digest.update(bytes);
        format!("{:x}", digest.finalize())
    };
    let objects = parse_objects(bytes);
    let mut page_objects = objects
        .values()
        .filter(|object| is_page_dictionary(&object.dictionary))
        .collect::<Vec<_>>();
    page_objects.sort_by_key(|object| object.id);
    let page_count = page_tree_count_hint(bytes).or_else(|| pdfium_page_count(path));

    // Some generated PDFs omit a conventional page dictionary while still
    // containing `/Type /Page` in raw bytes. Prefer PDFium's page tree for
    // object streams/xref streams, which the lightweight text parser does not
    // expand, and keep a clear error only when both parsers reject the file.
    if page_objects.is_empty() {
        let fallback_count = count_page_markers(bytes).max(1);
        if fallback_count > MAX_PDF_PAGES {
            return Err(format!(
                "PDF 页数超过上限（最多 {} 页，检测到 {} 页）",
                MAX_PDF_PAGES, fallback_count
            ));
        }
        if let Some(page_count) = page_count {
            if page_count > MAX_PDF_PAGES {
                return Err(format!(
                    "PDF 页数超过上限（最多 {} 页，检测到 {} 页）",
                    MAX_PDF_PAGES, page_count
                ));
            }
            // Object/xref streams are common in PDFs produced by modern
            // toolchains.  PDFium can still expose their text layer even
            // when the lightweight parser cannot see a conventional page
            // dictionary.  Prefer that native text over sending every page
            // through OCR (which was the source of the v4 garbage output).
            if let Some(document) = pdfium_text_document(path, bytes, page_count) {
                return Ok(document);
            }
            return Ok(fallback_document(
                path,
                bytes,
                page_count,
                "pdfium-page-tree",
            ));
        }
        return Err(format!(
            "PDF page tree is unsupported (detected {fallback_count} page marker(s))"
        ));
    }

    if page_objects.len() > MAX_PDF_PAGES {
        return Err(format!(
            "PDF 页数超过上限（最多 {} 页，检测到 {} 页）",
            MAX_PDF_PAGES,
            page_objects.len()
        ));
    }

    // Prefer PDFium's real text layer whenever it can open the document. It
    // preserves segment bounds and avoids the lightweight object parser
    // concatenating glyphs, running headers and footer text into false
    // paragraphs. The object parser remains the deterministic fallback for
    // PDFs PDFium cannot open or expose text for.
    if let Some(page_count) = page_count {
        if page_count <= MAX_PDF_PAGES {
            if let Some(document) = pdfium_text_document(path, bytes, page_count) {
                return Ok(document);
            }
        }
    }

    let mut pages = Vec::with_capacity(page_objects.len());
    let mut reading_order = Vec::new();
    let mut ocr_pages = Vec::new();
    let mut has_images = false;
    let mut has_tables = false;
    let mut has_formulas = false;

    for (index, page) in page_objects.iter().enumerate() {
        has_images |= contains_name(&page.dictionary, b"/Subtype", b"/Image");
        let page_text = page_text(page, &objects);
        let page_number = index as u32 + 1;
        let dimensions = media_box(&page.dictionary).unwrap_or((612.0, 792.0));
        let mut blocks = Vec::new();
        if page_text.is_empty() {
            ocr_pages.push(page_number);
        } else {
            for (block_index, line) in page_text.lines().enumerate() {
                let content = line.trim();
                if content.is_empty() {
                    continue;
                }
                let id = format!("p{page_number}-b{}", block_index + 1);
                reading_order.push(id.clone());
                let normalized = crate::document_text::normalize_text(content).0;
                let block_type = if content.contains('|') {
                    "table"
                } else if looks_like_formula(content) {
                    "formula"
                } else {
                    "text"
                };
                blocks.push(json!({
                    "id": id,
                    "type": block_type,
                    "content": normalized,
                    "rawText": content,
                    "source": "native/pdf",
                    "region": if block_type == "formula" { "formula" } else if block_type == "table" { "table" } else { "text" },
                    "language": detect_language(content),
                }));
            }
        }
        // Table/formula classification is deliberately conservative. A later
        // layout parser may enrich these blocks without changing this schema.
        has_tables |= page_text.contains('|');
        has_formulas |= page_text.contains("\\(")
            || page_text.contains("\\[")
            || page_text.lines().any(looks_like_formula);
        pages.push(json!({
            "number": page_number,
            "width": dimensions.0,
            "height": dimensions.1,
            "blocks": blocks,
        }));
    }

    let has_text_layer = !reading_order.is_empty();
    let route = match (has_text_layer, ocr_pages.is_empty()) {
        (true, true) => "native",
        (true, false) => "mixed",
        (false, false) => "ocr",
        (false, true) => "native",
    };
    let mut warnings = Vec::new();
    if !ocr_pages.is_empty() {
        warnings.push(json!({
            "code": "pdf-render-unavailable",
            "message": "扫描页需要 PDFium 渲染后交给 OCR Worker；解析任务会按页完成渲染与 OCR。"
        }));
    }

    let document_id = format!("pdf-{source_hash}");
    let document = json!({
        "id": document_id,
        "source": {
            "path": path,
            "mime": "application/pdf",
            "size": bytes.len(),
            "hash": source_hash,
            "engine": "native",
            "engineVersion": "builtin-pdf-text-1"
        },
        "metadata": {
            "pageCount": pages.len(),
            "hasTextLayer": has_text_layer,
            "isScanned": !has_text_layer,
            "hasTables": has_tables,
            "hasFormulas": has_formulas,
            "hasImages": has_images
        },
        "pages": pages,
        "structure": {
            "outline": [],
            "readingOrder": reading_order
        }
    });

    Ok(json!({
        "route": route,
        "requiresOcr": !ocr_pages.is_empty(),
        "ocrPageNumbers": ocr_pages,
        "warnings": warnings,
        "document": document
    }))
}

/// Extract native text from a PDFium page tree that is represented by object
/// streams/xref streams.  Each PDFium text segment becomes a normal text
/// block with its page-space bounding box, preserving enough layout metadata
/// for the converter and heading post-processor while avoiding OCR entirely.
fn pdfium_text_document(path: &str, bytes: &[u8], page_count: usize) -> Option<Value> {
    let pdfium = bind_pdfium().ok()?;
    let source = pdfium.load_pdf_from_file(path, None).ok()?;
    let mut pages = Vec::with_capacity(page_count);
    let mut reading_order = Vec::new();
    let mut has_text_layer = false;
    let mut has_formulas = false;
    for page_index in 0..page_count {
        let page = match source.pages().get(page_index as i32) {
            Ok(page) => page,
            Err(_) => {
                // A damaged page object must not truncate the document. Keep
                // the authoritative page count and let the OCR stage attempt
                // rendering; this also preserves stable page/chunk metadata.
                pages.push(json!({
                    "number": page_index + 1,
                    "width": 612.0,
                    "height": 792.0,
                    "blocks": []
                }));
                continue;
            }
        };
        let width = page.width().value;
        let height = page.height().value;
        let mut blocks = Vec::new();
        if let Ok(text) = page.text() {
            for (segment_index, segment) in text.segments().iter().enumerate() {
                let raw_content = segment.text().trim().to_string();
                if raw_content.is_empty() {
                    continue;
                }
                let content = crate::document_text::normalize_text(&raw_content).0;
                has_text_layer = true;
                has_formulas |= content.contains("\\(")
                    || content.contains("\\[")
                    || looks_like_formula(&content);
                let bounds = segment.bounds();
                let id = format!("p{}-b{}", page_index + 1, segment_index + 1);
                reading_order.push(id.clone());
                let block_type = native_block_type(&content);
                let mut block = json!({
                    "id": id,
                    "type": block_type,
                    "content": content,
                    "rawText": raw_content,
                    "source": "native/pdf",
                    "region": if block_type == "formula" { "formula" } else if block_type == "heading" { "heading" } else { "text" },
                    "language": detect_language(&content),
                    "bbox": [bounds.left().value, bounds.top().value, bounds.right().value, bounds.bottom().value],
                    "confidence": 1.0
                });
                // Pdfium segments can overlap at font changes and expose the
                // same character in two neighbouring segment strings. Keep
                // character identity and geometry for short math fragments so
                // reconstruction can deduplicate by source index instead of
                // deleting repeated text heuristically. Limiting this to
                // fragment-sized spans keeps large-book IR bounded.
                if content.chars().count() <= 4 {
                    if let Ok(chars) = segment.chars() {
                        let glyphs = chars
                            .iter()
                            .filter_map(|glyph| {
                                let character = glyph.unicode_char()?;
                                if character.is_whitespace() {
                                    return None;
                                }
                                let normalized = crate::document_text::normalize_text(
                                    &character.to_string(),
                                )
                                .0;
                                if normalized.is_empty() {
                                    return None;
                                }
                                let bounds = glyph.tight_bounds().ok()?;
                                Some(json!({
                                    "pageCharIndex": glyph.index(),
                                    "originalText": normalized,
                                    "rawCodepoint": character as u32,
                                    "bbox": [bounds.left().value, bounds.top().value, bounds.right().value, bounds.bottom().value],
                                    "baseline": glyph.origin_y().ok().map(|value| value.value),
                                    "fontName": glyph.font_name(),
                                    "fontSize": glyph.scaled_font_size().value,
                                    "coordinateSystem": "pdf",
                                }))
                            })
                            .collect::<Vec<_>>();
                        if !glyphs.is_empty() {
                            block["nativeGlyphs"] = Value::Array(glyphs);
                        }
                    }
                }
                blocks.push(block);
            }
        }
        coalesce_native_formula_fragments(&mut blocks);
        coalesce_native_multiline_math(&mut blocks);
        has_formulas |= blocks
            .iter()
            .any(|block| block["type"].as_str() == Some("formula"));
        pages.push(json!({
            "number": page_index + 1,
            "width": width,
            "height": height,
            "blocks": blocks
        }));
    }
    let source_hash = {
        let mut digest = Sha256::new();
        digest.update(bytes);
        format!("{:x}", digest.finalize())
    };
    Some(json!({
        "route": if has_text_layer { "native" } else { "ocr" },
        "requiresOcr": !has_text_layer,
        "ocrPageNumbers": if has_text_layer { json!([]) } else { json!((1..=page_count).collect::<Vec<_>>()) },
        "warnings": if has_text_layer { json!([]) } else { json!([{ "code": "pdfium-text-empty", "message": "PDFium 未发现原生文字层，页面将交给 OCR。" }]) },
        "document": {
            "id": format!("pdf-{source_hash}"),
            "source": {
                "path": path,
                "mime": "application/pdf",
                "size": bytes.len(),
                "hash": source_hash,
                "engine": "pdfium-text",
                "engineVersion": pdfium_bundled::PDFIUM_VERSION
            },
            "metadata": {
                "pageCount": page_count,
                "hasTextLayer": has_text_layer,
                "isScanned": !has_text_layer,
                "hasTables": false,
                "hasFormulas": has_formulas,
                "hasImages": bytes.windows(15).any(|window| window == b"/Subtype /Image")
                    || bytes.windows(14).any(|window| window == b"/Subtype/Image")
            },
            "pages": pages,
            "structure": { "outline": [], "readingOrder": reading_order }
        }
    }))
}

/// Reassemble PDFium font/style segments that belong to one visual text line.
/// Formula blocks remain atomic so downstream exporters can preserve OMML and
/// Markdown math instead of flattening them back into prose.
fn pdfium_page_count(path: &str) -> Option<usize> {
    let pdfium = bind_pdfium().ok()?;
    let document = pdfium.load_pdf_from_file(path, None).ok()?;
    usize::try_from(document.pages().len())
        .ok()
        .filter(|count| *count > 0)
}

/// Fallback envelope for PDFs whose page tree is represented by compressed
/// object/xref streams. PDFium provides the authoritative page count; pages
/// are explicitly routed to OCR so no fabricated text is returned.
fn fallback_document(path: &str, bytes: &[u8], page_count: usize, engine: &str) -> Value {
    let source_hash = {
        let mut digest = Sha256::new();
        digest.update(bytes);
        format!("{:x}", digest.finalize())
    };
    let pages = (1..=page_count)
        .map(|number| {
            json!({
                "number": number,
                "width": 612.0,
                "height": 792.0,
                "blocks": []
            })
        })
        .collect::<Vec<_>>();
    let ocr_page_numbers = (1..=page_count).collect::<Vec<_>>();
    json!({
        "route": "ocr",
        "requiresOcr": true,
        "ocrPageNumbers": ocr_page_numbers,
        "warnings": [{
            "code": "pdf-page-tree-fallback",
            "message": "PDF 页面树由 PDFium 解析；页面将渲染后交给 OCR Worker。"
        }],
        "document": {
            "id": format!("pdf-{source_hash}"),
            "source": {
                "path": path,
                "mime": "application/pdf",
                "size": bytes.len(),
                "hash": source_hash,
                "engine": engine,
                "engineVersion": pdfium_bundled::PDFIUM_VERSION
            },
            "metadata": {
                "pageCount": page_count,
                "hasTextLayer": false,
                "isScanned": true,
                "hasTables": false,
                "hasFormulas": false,
                "hasImages": false
            },
            "pages": pages,
            "structure": { "outline": [], "readingOrder": [] }
        }
    })
}

fn parse_objects(bytes: &[u8]) -> HashMap<u32, PdfObject> {
    let mut objects = HashMap::new();
    let mut cursor = 0usize;
    while objects.len() < MAX_OBJECTS {
        let Some(relative) = find_token(&bytes[cursor..], b"obj") else {
            break;
        };
        let obj_pos = cursor + relative;
        let Some((id, header_start)) = object_header(bytes, obj_pos) else {
            cursor = obj_pos + 3;
            continue;
        };
        let Some(end_relative) = find_token(&bytes[obj_pos + 3..], b"endobj") else {
            break;
        };
        let end_pos = obj_pos + 3 + end_relative;
        let object_body = &bytes[obj_pos + 3..end_pos];
        let stream = extract_stream(object_body);
        let dictionary_end = stream
            .as_ref()
            .and_then(|_| find_token(object_body, b"stream"))
            .unwrap_or(object_body.len());
        let dictionary = object_body[..dictionary_end].to_vec();
        objects.insert(
            id,
            PdfObject {
                id,
                dictionary,
                stream,
            },
        );
        // Keep the header variable meaningful for diagnostics/debuggers and
        // avoid re-scanning a long object body.
        cursor = header_start.max(end_pos + 6);
    }
    objects
}

fn object_header(bytes: &[u8], obj_pos: usize) -> Option<(u32, usize)> {
    let start = bytes[..obj_pos]
        .iter()
        .rposition(|byte| *byte == b'\n' || *byte == b'\r')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let line = std::str::from_utf8(&bytes[start..obj_pos]).ok()?;
    let mut tokens = line.split_whitespace();
    let id = tokens.next()?.parse::<u32>().ok()?;
    let _generation = tokens.next()?.parse::<u32>().ok()?;
    if tokens.next().is_some() {
        return None;
    }
    Some((id, start))
}

fn extract_stream(body: &[u8]) -> Option<Vec<u8>> {
    let stream_pos = find_token(body, b"stream")?;
    let end_relative = find_token(&body[stream_pos + 6..], b"endstream")?;
    let end_pos = stream_pos + 6 + end_relative;
    let mut start = stream_pos + 6;
    if body.get(start) == Some(&b'\r') {
        start += 1;
    }
    if body.get(start) == Some(&b'\n') {
        start += 1;
    }
    let mut raw = body[start..end_pos].to_vec();
    while matches!(raw.last(), Some(b'\r' | b'\n')) {
        raw.pop();
    }
    let dictionary = &body[..stream_pos];
    if contains_name(dictionary, b"/Filter", b"/FlateDecode") {
        let mut decoder = ZlibDecoder::new(raw.as_slice());
        let mut decoded = Vec::new();
        if decoder.read_to_end(&mut decoded).is_ok() {
            return Some(decoded);
        }
        return None;
    }
    Some(raw)
}

fn page_text(page: &PdfObject, objects: &HashMap<u32, PdfObject>) -> String {
    let references = references_after(&page.dictionary, b"/Contents");
    if references.is_empty() {
        return String::new();
    }
    let mut text = String::new();
    for reference in references {
        let Some(object) = objects.get(&reference) else {
            continue;
        };
        let Some(stream) = object.stream.as_ref() else {
            continue;
        };
        let part = extract_text(stream);
        if part.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&part);
    }
    text
}

fn extract_text(stream: &[u8]) -> String {
    let mut output = String::new();
    let mut cursor = 0usize;
    let mut line_break_pending = false;
    while cursor < stream.len() {
        match stream[cursor] {
            b'(' => {
                if let Some((raw, next)) = literal_string(stream, cursor) {
                    let operator = skip_ws(stream, next);
                    if token_at(stream, operator, b"Tj") {
                        append_text(&mut output, &raw, &mut line_break_pending);
                        cursor = operator + 2;
                        continue;
                    }
                }
            }
            b'[' => {
                if let Some((raw, next)) = text_array(stream, cursor) {
                    let operator = skip_ws(stream, next);
                    if token_at(stream, operator, b"TJ") {
                        append_text(&mut output, &raw, &mut line_break_pending);
                        cursor = operator + 2;
                        continue;
                    }
                }
            }
            b'<' if stream.get(cursor + 1) != Some(&b'<') => {
                if let Some((raw, next)) = hex_string(stream, cursor) {
                    let operator = skip_ws(stream, next);
                    if token_at(stream, operator, b"Tj") {
                        append_text(&mut output, &raw, &mut line_break_pending);
                        cursor = operator + 2;
                        continue;
                    }
                }
            }
            b'T' => {
                if token_at(stream, cursor, b"T*") {
                    line_break_pending = true;
                    cursor += 2;
                    continue;
                }
                if token_at(stream, cursor, b"Td") || token_at(stream, cursor, b"TD") {
                    line_break_pending = true;
                    cursor += 2;
                    continue;
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    normalize_text(&output)
}

fn text_array(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut cursor = start + 1;
    let mut depth = 1usize;
    let mut decoded = Vec::new();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'(' => {
                let (raw, next) = literal_string(bytes, cursor)?;
                decoded.extend(raw);
                cursor = next;
            }
            b'<' if bytes.get(cursor + 1) != Some(&b'<') => {
                let (raw, next) = hex_string(bytes, cursor)?;
                decoded.extend(raw);
                cursor = next;
            }
            b'[' => {
                depth += 1;
                cursor += 1;
            }
            b']' => {
                depth -= 1;
                cursor += 1;
                if depth == 0 {
                    return Some((decoded, cursor));
                }
            }
            _ => cursor += 1,
        }
    }
    None
}

fn literal_string(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut cursor = start + 1;
    let mut depth = 1usize;
    let mut decoded = Vec::new();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'(' => {
                depth += 1;
                decoded.push(b'(');
                cursor += 1;
            }
            b')' => {
                depth -= 1;
                cursor += 1;
                if depth == 0 {
                    return Some((decode_pdf_bytes(&decoded), cursor));
                }
                decoded.push(b')');
            }
            b'\\' => {
                cursor += 1;
                let Some(&escaped) = bytes.get(cursor) else {
                    break;
                };
                match escaped {
                    b'n' => decoded.push(b'\n'),
                    b'r' => decoded.push(b'\r'),
                    b't' => decoded.push(b'\t'),
                    b'b' => decoded.push(8),
                    b'f' => decoded.push(12),
                    b'(' | b')' | b'\\' => decoded.push(escaped),
                    b'\r' => {
                        cursor += 1;
                        if bytes.get(cursor) == Some(&b'\n') {
                            cursor += 1;
                        }
                        continue;
                    }
                    b'\n' => {}
                    byte if (b'0'..=b'7').contains(&byte) => {
                        // PDF octal escapes are specified as at most three
                        // digits, but malformed/generated files sometimes
                        // contain a value larger than one byte. Decode in a
                        // wider integer and clamp instead of allowing debug
                        // builds to panic on u8 overflow.
                        let mut value = u16::from(byte - b'0');
                        for _ in 0..2 {
                            if let Some(next) = bytes.get(cursor + 1) {
                                if (b'0'..=b'7').contains(next) {
                                    value = value
                                        .saturating_mul(8)
                                        .saturating_add(u16::from(*next - b'0'));
                                    cursor += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                        decoded.push(value.min(u16::from(u8::MAX)) as u8);
                    }
                    other => decoded.push(other),
                }
                cursor += 1;
            }
            byte => {
                decoded.push(byte);
                cursor += 1;
            }
        }
    }
    None
}

fn hex_string(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut cursor = start + 1;
    let mut nibbles = Vec::new();
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        cursor += 1;
        if byte == b'>' {
            if nibbles.len() % 2 != 0 {
                nibbles.push(0);
            }
            let mut decoded = Vec::with_capacity(nibbles.len() / 2);
            for pair in nibbles.as_chunks::<2>().0 {
                decoded.push((pair[0] << 4) | pair[1]);
            }
            return Some((decode_pdf_bytes(&decoded), cursor));
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        nibbles.push(hex_value(byte)?);
    }
    None
}

fn decode_pdf_bytes(bytes: &[u8]) -> Vec<u8> {
    let utf16_without_bom = bytes.len() >= 2
        && bytes.len().is_multiple_of(2)
        && bytes
            .as_chunks::<2>()
            .0
            .iter()
            .take(8)
            .any(|pair| pair[0] == 0);
    if (bytes.starts_with(&[0xfe, 0xff]) && bytes.len() >= 2) || utf16_without_bom {
        let mut result = String::new();
        let payload = if bytes.starts_with(&[0xfe, 0xff]) {
            &bytes[2..]
        } else {
            bytes
        };
        for pair in payload.as_chunks::<2>().0 {
            let code = u16::from_be_bytes([pair[0], pair[1]]);
            if let Some(character) = char::from_u32(code as u32) {
                result.push(character);
            }
        }
        return result.into_bytes();
    }
    bytes.to_vec()
}

fn append_text(output: &mut String, bytes: &[u8], line_break_pending: &mut bool) {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_matches(['\r', '\n']);
    if text.is_empty() {
        return;
    }
    if *line_break_pending && !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    *line_break_pending = false;
    output.push_str(text);
}

fn normalize_text(text: &str) -> String {
    let mut normalized = String::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !normalized.is_empty() {
            normalized.push('\n');
        }
        normalized.push_str(line);
    }
    normalized
}

fn media_box(dictionary: &[u8]) -> Option<(f64, f64)> {
    let marker = find_token(dictionary, b"/MediaBox")?;
    let start = skip_ws(dictionary, marker + 9);
    if dictionary.get(start) != Some(&b'[') {
        return None;
    }
    let values = dictionary[start + 1..]
        .split(|byte| byte.is_ascii_whitespace() || *byte == b']')
        .filter_map(|part| std::str::from_utf8(part).ok()?.parse::<f64>().ok())
        .take(4)
        .collect::<Vec<_>>();
    if values.len() == 4 {
        Some(((values[2] - values[0]).abs(), (values[3] - values[1]).abs()))
    } else {
        None
    }
}

fn references_after(dictionary: &[u8], name: &[u8]) -> Vec<u32> {
    let Some(marker) = find_token(dictionary, name) else {
        return Vec::new();
    };
    let mut cursor = skip_ws(dictionary, marker + name.len());
    let mut references = Vec::new();
    let limit = dictionary.len();
    while cursor < limit {
        if dictionary[cursor] == b']' {
            break;
        }
        if let Some((id, next)) = indirect_reference(dictionary, cursor) {
            references.push(id);
            cursor = next;
        } else {
            cursor += 1;
        }
        cursor = skip_ws(dictionary, cursor);
        if references.len() >= 32 {
            break;
        }
    }
    references
}

fn indirect_reference(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let mut cursor = start;
    let id_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if cursor == id_start {
        return None;
    }
    let id = std::str::from_utf8(&bytes[id_start..cursor])
        .ok()?
        .parse()
        .ok()?;
    cursor = skip_ws(bytes, cursor);
    let generation_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    if cursor == generation_start {
        return None;
    }
    cursor = skip_ws(bytes, cursor);
    if bytes.get(cursor..cursor + 1) != Some(b"R") {
        return None;
    }
    Some((id, cursor + 1))
}

fn is_page_dictionary(dictionary: &[u8]) -> bool {
    contains_name(dictionary, b"/Type", b"/Page") && !contains_name(dictionary, b"/Type", b"/Pages")
}

fn contains_name(bytes: &[u8], key: &[u8], value: &[u8]) -> bool {
    let Some(key_pos) = find_token(bytes, key) else {
        return false;
    };
    let value_pos = skip_ws(bytes, key_pos + key.len());
    bytes.get(value_pos..value_pos + value.len()) == Some(value)
}

fn count_page_markers(bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut cursor = 0;
    while let Some(relative) = find_token(&bytes[cursor..], b"/Type") {
        let position = cursor + relative;
        let value = skip_ws(bytes, position + 5);
        if bytes.get(value..value + 5) == Some(b"/Page") && bytes.get(value + 5) != Some(&b's') {
            count += 1;
        }
        cursor = position + 5;
    }
    count
}

/// Read the total page count from the PDF page tree when available. Some
/// scanned PDFs use compressed page objects that older PDFium builds expose
/// incompletely; the page-tree `/Count` keeps OCR from silently truncating
/// the document in that case.
fn page_tree_count_hint(bytes: &[u8]) -> Option<usize> {
    let mut cursor = 0usize;
    let mut best: Option<usize> = None;
    while let Some(relative) = find_token(&bytes[cursor..], b"/Type") {
        let position = cursor + relative;
        let value = skip_ws(bytes, position + 5);
        if bytes.get(value..value + 6) != Some(b"/Pages") {
            cursor = position + 5;
            continue;
        }
        let search_end = (value + 4096).min(bytes.len());
        let Some(relative_count) = find_token(&bytes[value..search_end], b"/Count") else {
            cursor = value + 6;
            continue;
        };
        let count_start = skip_ws(bytes, value + relative_count + 6);
        let mut count_end = count_start;
        while count_end < search_end && bytes[count_end].is_ascii_digit() {
            count_end += 1;
        }
        if count_end > count_start {
            if let Some(count) = std::str::from_utf8(&bytes[count_start..count_end])
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
            {
                if (1..=MAX_PDF_PAGES).contains(&count) {
                    best = Some(best.map_or(count, |current| current.max(count)));
                }
            }
        }
        cursor = value + 6;
    }
    best
}

fn detect_language(text: &str) -> &'static str {
    if text
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character))
    {
        "zh"
    } else {
        "en"
    }
}

fn find_token(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .find_map(|(offset, window)| {
            (window == needle && token_boundaries(haystack, offset, needle.len())).then_some(offset)
        })
}

fn token_boundaries(haystack: &[u8], offset: usize, len: usize) -> bool {
    let before = offset.checked_sub(1).and_then(|index| haystack.get(index));
    let after = haystack.get(offset + len);
    before.is_none_or(|byte| byte.is_ascii_whitespace() || b"[]<>()/%".contains(byte))
        && after.is_none_or(|byte| byte.is_ascii_whitespace() || b"[]<>()/%".contains(byte))
}

fn token_at(bytes: &[u8], position: usize, token: &[u8]) -> bool {
    bytes.get(position..position + token.len()) == Some(token)
        && bytes
            .get(position + token.len())
            .is_none_or(|byte| byte.is_ascii_whitespace() || b"[]<>()/%".contains(byte))
}

fn skip_ws(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    cursor
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_pdf(bytes: &[u8]) -> String {
        let dir = std::env::temp_dir().join(format!(
            "cb-pdf-parser-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("input.pdf");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn extracts_literal_text_and_pages() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<</Type /Page /MediaBox [0 0 200 100] /Contents 2 0 R>>\nendobj\n2 0 obj\n<</Length 23>>\nstream\nBT (Hello) Tj ET\nendstream\nendobj\n%%EOF";
        let path = write_pdf(pdf);
        let output = parse_file(&path).unwrap();
        assert_eq!(output["route"], "native");
        assert_eq!(output["document"]["metadata"]["pageCount"], 1);
        assert_eq!(
            output["document"]["pages"][0]["blocks"][0]["content"],
            "Hello"
        );
    }

    #[test]
    #[ignore = "requires DOCUMENT_ENGINE_SCAN_FIXTURE_PDF"]
    fn scan_fixture_preserves_page_tree_count() {
        let path = std::env::var("DOCUMENT_ENGINE_SCAN_FIXTURE_PDF").unwrap();
        let output = parse_file(&path).unwrap();
        let page_count = output["document"]["metadata"]["pageCount"]
            .as_u64()
            .unwrap_or_default();
        let page_array_count = output["document"]["pages"].as_array().map_or(0, Vec::len);
        eprintln!(
            "scan fixture page count: metadata={} pages={} ocrPageNumbers={}",
            page_count, page_array_count, output["ocrPageNumbers"]
        );
        assert_eq!(page_count, page_array_count as u64);
        assert_eq!(page_count, 10);
    }

    #[test]
    #[ignore = "requires DOCUMENT_ENGINE_GENERAL_FIXTURE_PDF"]
    fn general_fixture_parses_and_chunks() {
        let path = std::env::var("DOCUMENT_ENGINE_GENERAL_FIXTURE_PDF").unwrap();
        let mut output = parse_file(&path).unwrap();
        let document = output.get_mut("document").unwrap();
        let page_count = document["metadata"]["pageCount"]
            .as_u64()
            .unwrap_or_default();
        assert!(page_count > 0);
        let sanitization = crate::document_text::sanitize_document(document);
        crate::document_quality::annotate_native_text_quality(document);
        crate::document_structure::rebuild(document);
        cruciblebox_document::enrichment::enrich_formula_blocks(document);
        let quality =
            crate::document_quality::report(document, sanitization.invalid_control_chars_removed);
        let chunks = crate::document_chunker::chunk_document(document, None).unwrap();
        eprintln!(
            "general fixture: pages={} route={} quality={} chunks={}",
            page_count, output["route"], quality, chunks["count"]
        );
        assert_eq!(quality["invalidControlChars"], 0);
        assert!(chunks["count"].as_u64().unwrap_or_default() > 0);
    }

    #[test]
    fn extracts_escaped_and_hex_text() {
        let stream = b"BT (A\\(B) Tj [ (C) 120 (D) ] TJ <00480069> Tj ET";
        let text = extract_text(stream);
        assert!(text.contains("A(B"));
        assert!(text.contains("CD"));
        assert!(text.contains("Hi"));
    }

    #[test]
    fn scanned_page_is_explicitly_routed_to_ocr() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<</Type /Page /Contents 2 0 R>>\nendobj\n2 0 obj\n<</Length 4>>\nstream\nq Q\nendstream\nendobj\n%%EOF";
        let path = write_pdf(pdf);
        let output = parse_file(&path).unwrap();
        assert_eq!(output["route"], "ocr");
        assert_eq!(output["requiresOcr"], true);
        assert_eq!(output["ocrPageNumbers"][0], 1);
    }

    #[test]
    fn pdfium_fallback_preserves_page_count_without_fabricating_text() {
        let output = fallback_document("input.pdf", b"%PDF-1.7", 3, "pdfium-page-tree");
        assert_eq!(output["route"], "ocr");
        assert_eq!(output["document"]["metadata"]["pageCount"], 3);
        assert_eq!(output["ocrPageNumbers"], json!([1, 2, 3]));
        assert!(output["document"]["pages"][0]["blocks"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_fallback_documents_over_page_limit() {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        for _ in 0..(MAX_PDF_PAGES + 1) {
            pdf.extend_from_slice(b"/Type /Page\n");
        }
        let error = parse_bytes("input.pdf", &pdf).unwrap_err();
        assert!(error.contains("页数超过上限"));
    }

    #[cfg(windows)]
    #[test]
    fn pdfium_renders_embedded_scan_page() {
        let image = image::load_from_memory(include_bytes!("../../../../ocr-worker/test.png"))
            .expect("test image should decode")
            .into_rgb8();
        let pdf = make_image_pdf(&image);
        let path = write_pdf(&pdf);
        let output = std::env::temp_dir().join(format!(
            "cb-pdfium-render-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (width, height) = render_page_to_png(&path, 1, &output).unwrap();
        assert!(width > 0 && height > 0);
        assert!(std::fs::metadata(&output).unwrap().len() > 100);
        let (second_width, second_height) = render_page_to_png(&path, 1, &output).unwrap();
        assert_eq!((second_width, second_height), (width, height));
        let status = renderer_status();
        assert_eq!(status["initialized"], true);
        let _ = std::fs::remove_file(output);
    }

    #[cfg(windows)]
    #[test]
    fn pdfium_merge_and_reorder_create_readable_files() {
        let image = image::load_from_memory(include_bytes!("../../../../ocr-worker/test.png"))
            .unwrap()
            .into_rgb8();
        let source = write_pdf(&make_image_pdf(&image));
        let directory = Path::new(&source).parent().unwrap();
        let merged = directory.join("merged.pdf");
        let reordered = directory.join("reordered.pdf");
        let rotated = directory.join("rotated.pdf");
        let extracted_images = directory.join("images");
        let merge = merge_pdf_files(&[source.clone(), source.clone()], &merged).unwrap();
        assert_eq!(merge["pageCount"], 2);
        let reorder = reorder_pdf_pages(merged.to_str().unwrap(), &[2, 1], &reordered).unwrap();
        assert_eq!(reorder["pageCount"], 2);
        let pdfium = bind_pdfium().unwrap();
        assert_eq!(
            pdfium
                .load_pdf_from_file(&reordered, None)
                .unwrap()
                .pages()
                .len(),
            2
        );
        let rotation = rotate_pdf_pages(reordered.to_str().unwrap(), &[1], 90, &rotated).unwrap();
        assert_eq!(rotation["rotatedPages"], json!([1]));
        let rotated_pdf = pdfium.load_pdf_from_file(&rotated, None).unwrap();
        assert_eq!(
            rotated_pdf
                .pages()
                .get(0)
                .unwrap()
                .rotation()
                .unwrap()
                .as_degrees(),
            90.0
        );
        let images = extract_pdf_images(&source, &extracted_images).unwrap();
        assert_eq!(images["imageCount"], 1);
    }

    #[cfg(windows)]
    #[test]
    fn split_pdf_preserves_existing_output_and_validates_new_file() {
        let image = image::load_from_memory(include_bytes!("../../../../ocr-worker/test.png"))
            .unwrap()
            .into_rgb8();
        let source = write_pdf(&make_image_pdf(&image));
        let directory = Path::new(&source).parent().unwrap().join("split");
        std::fs::create_dir_all(&directory).unwrap();
        let existing = directory.join("input_001-001页.pdf");
        std::fs::write(&existing, b"keep this file").unwrap();
        let journal = directory.join("split-tasks.sqlite");
        let runtime = cruciblebox_task_runtime::TaskRuntime::open(&journal).unwrap();
        let result = runtime
            .run_sync("document-engine", "split", Some("pdf-split"), |ctx| {
                split_pdf_file_with_publication(&source, &directory, 1, Some(ctx))
            })
            .unwrap();
        let output = result["files"][0]["path"].as_str().unwrap();
        assert_ne!(Path::new(output), existing);
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep this file");
        assert_eq!(
            bind_pdfium()
                .unwrap()
                .load_pdf_from_file(output, None)
                .unwrap()
                .pages()
                .len(),
            1
        );

        let ranged = runtime
            .run_sync("document-engine", "split", Some("pdf-ranges"), |ctx| {
                split_pdf_file_with_ranges_with_publication(
                    &source,
                    &directory,
                    &[(1, 1)],
                    Some(ctx),
                )
            })
            .unwrap();
        let ranged_output = ranged["files"][0]["path"].as_str().unwrap();
        assert_ne!(ranged_output, output);
        drop(runtime);
        let reopened = cruciblebox_task_runtime::TaskRuntime::open(&journal).unwrap();
        assert_eq!(
            reopened.get("document-engine", "pdf-split").unwrap()["resultRefs"][0],
            directory.canonicalize().unwrap().to_string_lossy().as_ref()
        );
        assert_eq!(
            reopened.get("document-engine", "pdf-ranges").unwrap()["resultRefs"][0],
            directory.canonicalize().unwrap().to_string_lossy().as_ref()
        );
        assert_eq!(
            bind_pdfium()
                .unwrap()
                .load_pdf_from_file(ranged_output, None)
                .unwrap()
                .pages()
                .len(),
            1
        );
    }

    #[test]
    #[cfg(windows)]
    fn remaining_pdf_operations_publish_verified_results_and_many_parts_use_one_reference() {
        let image = image::RgbImage::from_pixel(4, 4, image::Rgb([30, 120, 210]));
        let source = write_pdf(&make_image_pdf(&image));
        let root = Path::new(&source).parent().unwrap();
        let journal = root.join("remaining-tasks.sqlite");
        let runtime = cruciblebox_task_runtime::TaskRuntime::open(&journal).unwrap();
        let merged = root.join("merged.pdf");
        runtime
            .run_sync("document-engine", "split", Some("merge"), |ctx| {
                merge_pdf_files_with_publication(
                    &[source.clone(), source.clone()],
                    &merged,
                    Some(ctx),
                )
            })
            .unwrap();
        let reordered = root.join("reordered.pdf");
        runtime
            .run_sync("document-engine", "split", Some("reorder"), |ctx| {
                reorder_pdf_pages_with_publication(
                    merged.to_str().unwrap(),
                    &[2, 1],
                    &reordered,
                    Some(ctx),
                )
            })
            .unwrap();
        let rotated = root.join("rotated.pdf");
        runtime
            .run_sync("document-engine", "split", Some("rotate"), |ctx| {
                rotate_pdf_pages_with_publication(
                    reordered.to_str().unwrap(),
                    &[1],
                    90,
                    &rotated,
                    Some(ctx),
                )
            })
            .unwrap();
        let images = root.join("images");
        let extracted = runtime
            .run_sync("document-engine", "split", Some("images"), |ctx| {
                extract_pdf_images_with_publication(&source, &images, Some(ctx))
            })
            .unwrap();
        assert_eq!(extracted["imageCount"], 1);
        image::open(extracted["files"][0]["path"].as_str().unwrap()).unwrap();
        let many = root.join("many.pdf");
        merge_pdf_files(&vec![source.clone(); 40], &many).unwrap();
        let parts = root.join("parts");
        let result = runtime
            .run_sync("document-engine", "split", Some("forty-parts"), |ctx| {
                split_pdf_file_with_publication(many.to_str().unwrap(), &parts, 1, Some(ctx))
            })
            .unwrap();
        assert_eq!(result["fileCount"], 40);
        for file in result["files"].as_array().unwrap() {
            assert!(Path::new(file["path"].as_str().unwrap()).is_file());
        }
        drop(runtime);
        let reopened = cruciblebox_task_runtime::TaskRuntime::open(&journal).unwrap();
        for (id, target) in [
            ("merge", merged),
            ("reorder", reordered),
            ("rotate", rotated),
            ("images", images),
            ("forty-parts", parts),
        ] {
            let snapshot = reopened.get("document-engine", id).unwrap();
            assert_eq!(snapshot["status"], "succeeded");
            assert_eq!(snapshot["resultRefs"].as_array().unwrap().len(), 1);
            let reference = Path::new(snapshot["resultRefs"][0].as_str().unwrap())
                .canonicalize()
                .unwrap();
            assert_eq!(reference, target.canonicalize().unwrap());
        }
    }
    #[cfg(windows)]
    fn make_image_pdf(image: &image::RgbImage) -> Vec<u8> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(image.as_raw()).unwrap();
        let compressed = encoder.finish().unwrap();
        let content = format!(
            "q\n{} 0 0 {} 0 0 cm\n/Im1 Do\nQ\n",
            image.width(),
            image.height()
        );
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>",
                image.width(),
                image.height()
            )
            .into_bytes(),
            format!(
                "<< /Length {} >>\nstream\n{}endstream",
                content.len(),
                content
            )
            .into_bytes(),
            {
                let mut object = format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                    image.width(),
                    image.height(),
                    compressed.len()
                )
                .into_bytes();
                object.extend_from_slice(&compressed);
                object.extend_from_slice(b"\nendstream");
                object
            },
        ];
        let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = vec![0usize];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes(),
        );
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                offsets.len()
            )
            .as_bytes(),
        );
        pdf
    }

    #[test]
    fn coalesces_same_line_math_fragments_but_not_document_labels() {
        let mut blocks = vec![
            json!({"id":"a","type":"text","content":"A","bbox":[10,100,18,112]}),
            json!({"id":"t","type":"text","content":"T","bbox":[22,100,30,112]}),
            json!({"id":"ax","type":"text","content":"A","bbox":[34,100,42,112]}),
            json!({"id":"x","type":"text","content":"x","bbox":[46,100,54,112]}),
            json!({"id":"eq","type":"text","content":"=","bbox":[60,100,68,112]}),
            json!({"id":"b","type":"text","content":"b","bbox":[74,100,82,112]}),
            json!({"id":"label","type":"text","content":"FIELD ARCHIVE / FOGHARBOR","bbox":[10,20,220,32]}),
        ];
        coalesce_native_formula_fragments(&mut blocks);
        assert_eq!(blocks[0]["type"], "formula");
        assert_eq!(blocks[0]["content"], "A T A x = b");
        assert_eq!(blocks[1]["content"], "FIELD ARCHIVE / FOGHARBOR");
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn formula_coalescing_stops_at_prose_and_marks_mixed_line_inline() {
        let mut blocks = vec![
            json!({"id":"prefix","type":"text","content":"Solving","bbox":[0,100,38,112]}),
            json!({"id":"a","type":"text","content":"A","bbox":[42,100,48,112]}),
            json!({"id":"x","type":"text","content":"x","bbox":[49,100,55,112]}),
            json!({"id":"eq","type":"text","content":"=","bbox":[57,102,63,110]}),
            json!({"id":"b","type":"text","content":"b","bbox":[65,100,71,112]}),
            json!({"id":"and","type":"text","content":"and","bbox":[75,100,93,112]}),
            json!({"id":"c","type":"text","content":"c","bbox":[97,100,103,112]}),
        ];
        coalesce_native_formula_fragments(&mut blocks);
        assert_eq!(blocks[1]["content"], "A x = b");
        assert_eq!(blocks[1]["displayOrInline"], "inline");
        assert_eq!(blocks[2]["content"], "and");
    }

    #[test]
    fn coalescing_removes_only_geometrically_overlapping_duplicate_glyphs() {
        let mut blocks = vec![
            json!({"id":"x1","type":"text","content":"x","bbox":[10,100,18,112]}),
            json!({"id":"x-overlay","type":"text","content":"x","bbox":[10.2,100,18.2,112]}),
            json!({"id":"plus","type":"text","content":"+","bbox":[24,100,32,112]}),
            json!({"id":"x2","type":"text","content":"x","bbox":[38,100,46,112]}),
            json!({"id":"eq","type":"text","content":"=","bbox":[52,100,60,112]}),
            json!({"id":"two","type":"text","content":"2","bbox":[66,100,74,112]}),
        ];
        coalesce_native_formula_fragments(&mut blocks);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["content"], "x + x = 2");
        assert_eq!(blocks[0]["originalTokens"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn coalescing_removes_smaller_duplicate_window_at_segment_boundary() {
        let mut blocks = vec![
            json!({"id":"lhs","type":"text","content":"A","bbox":[10,100,18,112]}),
            json!({"id":"eq","type":"text","content":"=","bbox":[22,104,30,108]}),
            json!({"id":"lambda-x","type":"text","content":"λx","bbox":[34,100,46,112]}),
            json!({"id":"lambda-x-window","type":"text","content":"λx","bbox":[45.8,103,52,110]}),
        ];
        coalesce_native_formula_fragments(&mut blocks);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["content"], "A = λx");
        assert_eq!(blocks[0]["originalTokens"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn coalesces_native_text_style_fragments_on_the_same_line() {
        let mut blocks = vec![
            json!({"id":"a","type":"text","content":"The equation 2x","bbox":[10.0,20.0,90.0,32.0]}),
            json!({"id":"b","type":"text","content":"−","bbox":[91.0,20.0,96.0,32.0]}),
            json!({"id":"c","type":"text","content":"y = 1 is represented","bbox":[97.0,20.0,190.0,32.0]}),
        ];
        coalesce_native_text_fragments(&mut blocks);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["content"], "The equation 2x−y = 1 is represented");
    }

    #[test]
    fn native_text_fragment_merge_preserves_distinct_columns() {
        let mut blocks = vec![
            json!({"id":"a","type":"text","content":"left","bbox":[10.0,20.0,40.0,32.0]}),
            json!({"id":"b","type":"text","content":"right","bbox":[300.0,20.0,340.0,32.0]}),
        ];
        coalesce_native_text_fragments(&mut blocks);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn native_text_fragment_merge_removes_overlapping_suffix_window() {
        let mut blocks = vec![
            json!({"id":"a","type":"paragraph","content":"The equation 2x","bbox":[10.0,20.0,90.0,32.0]}),
            json!({"id":"b","type":"paragraph","content":"2x","bbox":[89.5,21.0,101.0,31.0]}),
            json!({"id":"c","type":"paragraph","content":"− y = 1","bbox":[102.0,20.0,140.0,32.0]}),
        ];
        coalesce_native_text_fragments(&mut blocks);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["content"], "The equation 2x− y = 1");
    }

    #[test]
    fn coalesces_aligned_equations_but_not_unrelated_formula_lines() {
        let mut equations = vec![
            json!({"id":"r1","type":"formula","content":"x + y = 2","bbox":[20,100,100,112]}),
            json!({"id":"r2","type":"formula","content":"x - y = 0","bbox":[20,116,100,128]}),
        ];
        coalesce_native_multiline_math(&mut equations);
        assert_eq!(equations.len(), 1);
        assert_eq!(equations[0]["content"], "x + y = 2\nx - y = 0");

        let mut unrelated = vec![
            json!({"id":"a","type":"formula","content":"x + y","bbox":[20,100,100,112]}),
            json!({"id":"b","type":"formula","content":"a - b","bbox":[20,116,100,128]}),
        ];
        coalesce_native_multiline_math(&mut unrelated);
        assert_eq!(unrelated.len(), 2);
    }

    #[test]
    fn rejects_non_pdf() {
        let path = write_pdf(b"not a pdf");
        assert!(parse_file(&path).is_err());
    }

    #[test]
    #[ignore = "requires DOCUMENT_ENGINE_FIXTURE_PDF pointing at the local large PDF"]
    fn parses_large_fixture_without_forcing_ocr() {
        let path = std::env::var("DOCUMENT_ENGINE_FIXTURE_PDF").unwrap();
        let parsed = parse_file(&path).unwrap();
        let document = parsed.get("document").unwrap();
        // The regression fixture is intentionally supplied by the caller and
        // has changed from the original 542-page sample to Thomas Calculus
        // (1348 pages). Validate the bounded real page count instead of
        // coupling the parser to one historical copy.
        assert!(document["metadata"]["pageCount"].as_u64().unwrap_or(0) >= 500);
        assert!(document["metadata"]["hasTextLayer"]
            .as_bool()
            .unwrap_or(false));
        assert!(!parsed["requiresOcr"].as_bool().unwrap_or(true));
        let mut normalized = document.clone();
        let sanitization = crate::document_text::sanitize_document(&mut normalized);
        crate::document_quality::annotate_native_text_quality(&mut normalized);
        crate::document_structure::rebuild(&mut normalized);
        for page in normalized["pages"].as_array_mut().into_iter().flatten() {
            if let Some(blocks) = page["blocks"].as_array_mut() {
                coalesce_native_text_fragments(blocks);
            }
        }
        cruciblebox_document::enrichment::enrich_formula_blocks(&mut normalized);
        let quality = crate::document_quality::report(
            &normalized,
            sanitization.invalid_control_chars_removed,
        );
        eprintln!("fixture document quality: {quality}");
        let formula_samples = normalized["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|page| page["blocks"].as_array().into_iter().flatten())
            .filter(|block| block["type"] == "formula")
            .filter_map(|block| block["content"].as_str())
            .take(40)
            .collect::<Vec<_>>();
        eprintln!("fixture formula samples: {formula_samples:?}");
        assert_eq!(quality["invalidControlChars"], 0);
        assert_eq!(quality["invalidXmlChars"], 0);
        assert!(
            quality["matrixBlockCount"].as_u64().unwrap_or(0) > 0,
            "math textbook should recover at least one structured matrix"
        );
        let chunks = crate::document_chunker::chunk_document(&normalized, None).unwrap();
        eprintln!("fixture chunk quality: {}", chunks["quality"]);
        assert!(chunks["quality"]["passed"].as_bool().unwrap_or(false));
        let output_root = std::env::temp_dir().join(format!(
            "cruciblebox-math-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&output_root).unwrap();
        let markdown_path = output_root.join("document.md");
        let docx_path = output_root.join("document.docx");
        let markdown =
            crate::document_converter::convert_document(&normalized, "md", markdown_path.to_str())
                .unwrap();
        let docx =
            crate::document_converter::convert_document(&normalized, "docx", docx_path.to_str())
                .unwrap();
        let markdown_text = std::fs::read_to_string(&markdown_path).unwrap();
        assert!(markdown_text.contains("$$") || markdown_text.contains('$'));
        assert_eq!(docx["quality"]["docxXmlParse"], "passed");
        assert_eq!(docx["quality"]["invalidXmlChars"], 0);
        assert!(markdown["bytes"].as_u64().unwrap_or(0) > 0);
        assert!(docx["bytes"].as_u64().unwrap_or(0) > 0);
        eprintln!(
            "fixture exports: matrixBlocks={} markdownBytes={} docxBytes={} docxQuality={}",
            quality["matrixBlockCount"], markdown["bytes"], docx["bytes"], docx["quality"]
        );
        if std::env::var_os("DOCUMENT_ENGINE_KEEP_OUTPUT").is_some() {
            std::fs::write(
                output_root.join("document.json"),
                serde_json::to_vec_pretty(&normalized).unwrap(),
            )
            .unwrap();
            std::fs::write(
                output_root.join("document-chunks.json"),
                serde_json::to_vec_pretty(&chunks).unwrap(),
            )
            .unwrap();
            eprintln!("fixture output retained at {}", output_root.display());
        } else {
            let _ = std::fs::remove_dir_all(output_root);
        }
    }
}
