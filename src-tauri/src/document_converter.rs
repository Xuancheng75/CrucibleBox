//! Converters built on top of the unified Document model.
//!
//! Text/Markdown/HTML/JSON and lightweight DOCX/PDF writers are implemented
//! locally.  The generated DOCX/PDF intentionally uses a small, portable
//! subset; layout-heavy authoring remains a future specialized backend.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

/// Parse the source document and write a converted representation.
#[allow(dead_code)]
pub fn convert_file(path: &str, target: &str, output_path: Option<&str>) -> Result<Value, String> {
    convert_file_with_cache(path, target, output_path, None)
}

#[allow(dead_code)]
pub fn convert_file_with_cache(
    path: &str,
    target: &str,
    output_path: Option<&str>,
    cache_directory: Option<&str>,
) -> Result<Value, String> {
    let parsed = crate::document_parser::parse_file(path)?;
    let document = parsed
        .get("document")
        .ok_or_else(|| "解析器未返回 Document".to_string())?;
    convert_document_with_cache(document, target, output_path, cache_directory)
}

#[allow(dead_code)]
pub fn convert_document(
    document: &Value,
    target: &str,
    output_path: Option<&str>,
) -> Result<Value, String> {
    convert_document_with_cache(document, target, output_path, None)
}

pub fn convert_document_with_cache(
    document: &Value,
    target: &str,
    output_path: Option<&str>,
    cache_directory: Option<&str>,
) -> Result<Value, String> {
    let target = normalize_target(target)?;
    let cache_key = crate::document_engine_cache::cache_key(
        document["source"]["hash"].as_str().unwrap_or(""),
        document["source"]["engine"].as_str().unwrap_or("native"),
        document["source"]["engineVersion"].as_str().unwrap_or("1"),
        &json!({ "target": target }),
    );
    let source_path = document
        .get("source")
        .and_then(|source| source.get("path"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let destination = output_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(source_path, target));
    if destination.exists() && destination.is_dir() {
        return Err(format!(
            "转换输出路径是文件夹，请选择一个文件名：{}",
            destination.to_string_lossy()
        ));
    }
    if same_path(source_path, &destination) {
        return Err("输出路径不能覆盖输入文件".into());
    }
    if let Some(cache_directory) = cache_directory {
        if let Ok(Some(cached)) =
            crate::document_engine_cache::read_result(Path::new(cache_directory), &cache_key)
        {
            if cached
                .get("outputPath")
                .and_then(Value::as_str)
                .is_some_and(|path| Path::new(path).is_file())
            {
                return Ok(cached);
            }
        }
    }
    if let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建输出目录失败: {error}"))?;
        ensure_writable_directory(parent)?;
    }
    let bytes = match target {
        "txt" => document_to_text(document).into_bytes(),
        "md" => document_to_markdown(document).into_bytes(),
        "html" => document_to_html(document).into_bytes(),
        "json" => serde_json::to_vec_pretty(document)
            .map_err(|error| format!("序列化 Document JSON 失败: {error}"))?,
        "docx" => document_to_docx(document)?,
        "pdf" => document_to_pdf_with_source(document),
        _ => unreachable!("normalize_target validates target"),
    };
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err("转换输出超过大小限制".into());
    }
    let committed = write_atomic_bytes(&destination, &bytes)?;
    let result = json!({
        "outputPath": committed.to_string_lossy(),
        "target": target,
        "bytes": bytes.len(),
        "cacheKey": cache_key,
        "documentId": document.get("id").cloned().unwrap_or(Value::Null),
        "quality": if target == "docx" {
            json!({
                "invalidXmlChars": 0,
                "docxXmlParse": "passed",
                "docxOpenTest": "not_run"
            })
        } else {
            Value::Null
        },
        "warnings": if target == "pdf" { json!(["PDF 输出使用内置文本布局，复杂布局可能降级"]) } else { json!([]) }
    });
    if let Some(cache_directory) = cache_directory {
        let _ = crate::document_engine_cache::write_result(
            Path::new(cache_directory),
            &cache_key,
            &result,
        );
    }
    Ok(result)
}

/// Export the complete parsed document to bounded, AI-friendly files. The
/// task RPC only returns metadata and a preview for large documents; callers
/// should use these paths for the full content.
pub fn export_document_bundle(
    document: &Value,
    output_directory: &Path,
    stem: &str,
) -> Result<Value, String> {
    std::fs::create_dir_all(output_directory)
        .map_err(|error| format!("创建解析输出目录失败: {error}"))?;
    ensure_writable_directory(output_directory)?;
    let safe_stem = sanitize_stem(stem);
    let files = [
        (
            "markdown",
            "md",
            document_to_markdown(document).into_bytes(),
        ),
        ("text", "txt", document_to_text(document).into_bytes()),
        (
            "document",
            "document.json",
            serde_json::to_vec_pretty(document)
                .map_err(|error| format!("序列化解析结果失败: {error}"))?,
        ),
    ];
    let mut outputs = Vec::with_capacity(files.len());
    for (kind, extension, bytes) in files {
        if bytes.len() > MAX_OUTPUT_BYTES {
            return Err(format!("{kind} 解析输出超过大小限制"));
        }
        let destination = output_directory.join(format!("{safe_stem}.{extension}"));
        let output_path = write_atomic_bytes(&destination, &bytes)?;
        outputs.push(json!({
            "kind": kind,
            "path": output_path.to_string_lossy(),
            "bytes": bytes.len()
        }));
    }
    Ok(json!({
        "directory": output_directory.to_string_lossy(),
        "files": outputs,
        "textCharacters": document_to_text(document).chars().count()
    }))
}

fn sanitize_stem(value: &str) -> String {
    let trimmed = value.trim();
    let mut result = trimmed
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    while result.ends_with('.') || result.ends_with(' ') {
        result.pop();
    }
    if result.is_empty() {
        "document".into()
    } else {
        result
    }
}

fn same_path(source: &str, destination: &Path) -> bool {
    let source_path = Path::new(source);
    if source_path == destination {
        return true;
    }
    match (source_path.canonicalize(), destination.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn ensure_writable_directory(directory: &Path) -> Result<(), String> {
    let probe = directory.join(format!(
        ".cruciblebox-write-test-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            Ok(())
        }
        Err(error) => Err(format!(
            "输出目录不可写：{} ({error})",
            directory.to_string_lossy()
        )),
    }
}

fn write_atomic_bytes(destination: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    if destination.exists() && destination.is_dir() {
        return Err(format!(
            "输出目标是文件夹：{}",
            destination.to_string_lossy()
        ));
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建输出目录失败: {error}"))?;
        ensure_writable_directory(parent)?;
    }
    let temp_path = destination.with_extension(format!(
        "{}.{}.tmp",
        destination
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("out"),
        unique_suffix()
    ));
    if temp_path.exists() {
        std::fs::remove_file(&temp_path)
            .map_err(|error| format!("清理输出临时文件失败: {error}"))?;
    }
    std::fs::write(&temp_path, bytes).map_err(|error| format!("写入输出文件失败: {error}"))?;
    let target = if destination.exists() {
        match std::fs::remove_file(destination) {
            Ok(()) => destination.to_path_buf(),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
                ) =>
            {
                // Windows refuses to replace a file that is still open by a
                // previewer/editor.  Preserve the completed result under a
                // deterministic sibling instead of reporting a generic
                // "os error 5" after a long conversion.
                next_available_sibling(destination).ok_or_else(|| {
                    format!("输出文件被其他程序占用，且无法生成备用文件名：{error}")
                })?
            }
            Err(error) => return Err(format!("替换已有输出文件失败: {error}")),
        }
    } else {
        destination.to_path_buf()
    };
    if let Err(error) = std::fs::rename(&temp_path, &target) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("提交输出文件失败: {error}"));
    }
    Ok(target)
}

fn next_available_sibling(destination: &Path) -> Option<PathBuf> {
    let stem = destination.file_stem()?.to_string_lossy();
    let extension = destination.extension().map(|value| value.to_string_lossy());
    for index in 1..=99u32 {
        let name = match &extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = destination.with_file_name(name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_else(|_| u128::from(std::process::id()))
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

fn default_output_path(source_path: &str, target: &str) -> PathBuf {
    let source = Path::new(source_path);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}.{target}"))
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

type DocumentBlock = (
    usize,
    String,
    String,
    Option<String>,
    Option<u8>,
    Option<String>,
    Option<crate::document_math::MathNode>,
);

fn document_blocks(document: &Value) -> Vec<DocumentBlock> {
    let mut blocks: Vec<DocumentBlock> = Vec::new();
    if let Some(pages) = document.get("pages").and_then(Value::as_array) {
        for page in pages {
            let number = page.get("number").and_then(Value::as_u64).unwrap_or(1) as usize;
            let mut previous_source_bbox = None;
            let mut paragraph_left = None;
            if let Some(page_blocks) = page.get("blocks").and_then(Value::as_array) {
                for block in page_blocks {
                    let Some(content) = block.get("content").and_then(Value::as_str) else {
                        continue;
                    };
                    if !content.trim().is_empty() {
                        let block_type = block
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("paragraph")
                            .to_string();
                        let bbox = converter_block_bbox(block);
                        let merge_plain_text = matches!(block_type.as_str(), "text" | "paragraph")
                            && blocks.last().is_some_and(|previous: &DocumentBlock| {
                                previous.0 == number
                                    && matches!(previous.1.as_str(), "text" | "paragraph")
                                    && adjacent_text_geometry(
                                        previous_source_bbox,
                                        bbox,
                                        paragraph_left,
                                    )
                            });
                        if merge_plain_text {
                            let previous = blocks.last_mut().expect("checked above");
                            if previous.2.ends_with('-')
                                && content.trim_start().starts_with(char::is_lowercase)
                            {
                                previous.2.pop();
                            } else if !previous.2.ends_with(char::is_whitespace) {
                                previous.2.push(' ');
                            }
                            previous.2.push_str(content.trim());
                            previous_source_bbox = bbox;
                            continue;
                        }
                        blocks.push((
                            number,
                            block_type,
                            content.trim().to_string(),
                            crate::document_math::latex_from_block(block).or_else(|| {
                                block
                                    .get("latex")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                            }),
                            block
                                .get("level")
                                .and_then(Value::as_u64)
                                .map(|value| value.clamp(1, 6) as u8),
                            block
                                .get("displayOrInline")
                                .and_then(Value::as_str)
                                .map(ToOwned::to_owned),
                            crate::document_math::ast_from_block(block),
                        ));
                        previous_source_bbox = bbox;
                        paragraph_left = bbox.map(|value| value[0]);
                    }
                }
            }
        }
    }
    blocks
}

fn converter_block_bbox(block: &Value) -> Option<[f64; 4]> {
    let values = block.get("bbox")?.as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()?,
        values[1].as_f64()?,
        values[2].as_f64()?,
        values[3].as_f64()?,
    ])
}

fn adjacent_text_geometry(
    left: Option<[f64; 4]>,
    right: Option<[f64; 4]>,
    paragraph_left: Option<f64>,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    let (left_y0, left_y1) = (left[1].min(left[3]), left[1].max(left[3]));
    let (right_y0, right_y1) = (right[1].min(right[3]), right[1].max(right[3]));
    let left_height = (left_y1 - left_y0).max(1.0);
    let right_height = (right_y1 - right_y0).max(1.0);
    let height = left_height.max(right_height);
    let overlap = (left_y1.min(right_y1) - left_y0.max(right_y0)).max(0.0);
    let same_line =
        overlap / left_height.min(right_height) >= 0.45 && right[0] - left[2] <= height * 4.0;
    let center_delta = ((left_y0 + left_y1) - (right_y0 + right_y1)).abs() / 2.0;
    let expected_left = paragraph_left.unwrap_or(left[0]);
    let next_line =
        center_delta <= height * 1.8 && (expected_left - right[0]).abs() <= height * 2.5;
    same_line || next_line
}

fn same_line_geometry(left: Option<[f64; 4]>, right: Option<[f64; 4]>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    let (left_y0, left_y1) = (left[1].min(left[3]), left[1].max(left[3]));
    let (right_y0, right_y1) = (right[1].min(right[3]), right[1].max(right[3]));
    let minimum_height = (left_y1 - left_y0).min(right_y1 - right_y0).max(1.0);
    let maximum_height = (left_y1 - left_y0).max(right_y1 - right_y0).max(1.0);
    let overlap = (left_y1.min(right_y1) - left_y0.max(right_y0)).max(0.0);
    overlap / minimum_height >= 0.35 && right[0] - left[2] <= maximum_height * 4.0
}

fn document_to_text(document: &Value) -> String {
    document_blocks(document)
        .into_iter()
        .map(|(_, block_type, content, _, _, _, ast)| {
            if matches!(block_type.as_str(), "formula" | "matrix") {
                ast.as_ref()
                    .map(crate::document_math::to_plain_text)
                    .unwrap_or(content)
            } else {
                content
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn document_to_markdown(document: &Value) -> String {
    let mut result = String::new();
    let mut previous_was_text = false;
    let mut pending_inline_formula = false;
    let mut previous_page = None;
    for (page, block_type, content, latex, level, display, _) in document_blocks(document) {
        if previous_page.is_some() && previous_page != Some(page) && pending_inline_formula {
            result.push_str("\n\n");
            pending_inline_formula = false;
        }
        previous_page = Some(page);
        match block_type.as_str() {
            "heading" => {
                if pending_inline_formula {
                    result.push_str("\n\n");
                }
                // Keep chapter/section hierarchy readable while remaining
                // compatible with blocks produced by older parser versions.
                let lower = content.to_ascii_lowercase();
                let level = level.map(usize::from).unwrap_or_else(|| {
                    if lower.starts_with("chapter ") || content.trim_start().starts_with('第') {
                        1
                    } else {
                        2
                    }
                });
                result.push_str(&"#".repeat(level));
                result.push(' ');
                result.push_str(&content);
            }
            "formula" | "matrix" => {
                let formula = latex.unwrap_or_else(|| normalize_formula(&content));
                if display.as_deref() == Some("inline") && block_type == "formula" {
                    if previous_was_text && result.ends_with("\n\n") {
                        result.truncate(result.len() - 2);
                        result.push(' ');
                    } else if pending_inline_formula {
                        result.push(' ');
                    }
                    result.push('$');
                    result.push_str(&formula);
                    result.push('$');
                    pending_inline_formula = true;
                    previous_was_text = false;
                    continue;
                } else {
                    if pending_inline_formula {
                        result.push_str("\n\n");
                    }
                    result.push_str("$$\n");
                    result.push_str(&formula);
                    result.push_str("\n$$");
                }
            }
            _ => {
                if pending_inline_formula {
                    result.push(' ');
                }
                result.push_str(&content);
            }
        }
        result.push_str("\n\n");
        previous_was_text = matches!(block_type.as_str(), "text" | "paragraph");
        pending_inline_formula = false;
    }
    if pending_inline_formula {
        result.push_str("\n\n");
    }
    result
}

fn normalize_formula(value: &str) -> String {
    crate::formula_ocr::recognize_text(value).latex
}

fn document_to_html(document: &Value) -> String {
    let mut result = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Document</title><style>body{max-width:980px;margin:2rem auto;padding:0 2rem;font-family:system-ui,sans-serif;line-height:1.65;color:#202124}.page{break-after:page;position:relative}.formula{font-family:serif;text-align:center;margin:1rem 0;font-size:1.1em}.caption{color:#666;font-size:.9em}</style></head><body>\n",
    );
    let mut current_page = None;
    for (page, block_type, content, latex, level, _, _) in document_blocks(document) {
        if current_page != Some(page) {
            if current_page.is_some() {
                result.push_str("</section>\n");
            }
            result.push_str(&format!("<section class=\"page\" data-page=\"{page}\">\n"));
            current_page = Some(page);
        }
        let tag = if block_type == "heading" {
            match level.unwrap_or(2) {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                5 => "h5",
                _ => "h6",
            }
        } else if matches!(block_type.as_str(), "formula" | "matrix") {
            "div"
        } else {
            "p"
        };
        result.push('<');
        result.push_str(tag);
        if matches!(block_type.as_str(), "formula" | "matrix") {
            result.push_str(" class=\"formula\" data-latex=\"");
            result.push_str(&escape_html(latex.as_deref().unwrap_or(&content)));
            result.push('"');
        }
        result.push('>');
        result.push_str(&escape_html(
            if matches!(block_type.as_str(), "formula" | "matrix") {
                latex.as_deref().unwrap_or(&content)
            } else {
                &content
            },
        ));
        result.push_str("</");
        result.push_str(tag);
        result.push_str(">\n");
    }
    if current_page.is_some() {
        result.push_str("</section>\n");
    }
    result.push_str("</body></html>\n");
    result
}

fn document_to_docx(document: &Value) -> Result<Vec<u8>, String> {
    let mut sanitized = document.clone();
    let _sanitization = crate::document_text::sanitize_document(&mut sanitized);
    let document_xml = document_to_docx_xml(&sanitized);
    validate_xml(&document_xml)?;
    let mut buffer = Vec::new();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("[Content_Types].xml", options)
        .map_err(|error| format!("创建 DOCX 内容类型失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>"#)
        .map_err(|error| error.to_string())?;
    writer
        .start_file("_rels/.rels", options)
        .map_err(|error| format!("创建 DOCX 关系失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#)
        .map_err(|error| error.to_string())?;
    writer
        .start_file("word/document.xml", options)
        .map_err(|error| format!("创建 DOCX 文档失败: {error}"))?;
    writer
        .write_all(document_xml.as_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .start_file("word/_rels/document.xml.rels", options)
        .map_err(|error| format!("创建 DOCX 文档关系失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rIdNumbering" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/><Relationship Id="rIdHeader" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rIdFooter" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#)
        .map_err(|error| error.to_string())?;
    writer
        .start_file("word/styles.xml", options)
        .map_err(|error| format!("创建 DOCX 样式失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="20"/><w:szCs w:val="20"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="120" w:after="60"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="100" w:after="40"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="80" w:after="40"/></w:pPr><w:rPr><w:b/><w:sz w:val="25"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:left="720"/></w:pPr></w:style><w:style w:type="paragraph" w:styleId="Caption"><w:name w:val="caption"/><w:basedOn w:val="Normal"/><w:rPr><w:i/><w:color w:val="666666"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Formula"><w:name w:val="Formula"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/><w:spacing w:before="0" w:after="0"/></w:pPr></w:style></w:styles>"#)
        .map_err(|error| error.to_string())?;
    writer
        .start_file("word/numbering.xml", options)
        .map_err(|error| format!("创建 DOCX 编号失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:start w:val="1"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#)
        .map_err(|error| error.to_string())?;
    writer
        .start_file("word/header1.xml", options)
        .map_err(|error| format!("创建 DOCX 页眉失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>CrucibleBox Document Engine</w:t></w:r></w:p></w:hdr>"#)
        .map_err(|error| error.to_string())?;
    writer
        .start_file("word/footer1.xml", options)
        .map_err(|error| format!("创建 DOCX 页脚失败: {error}"))?;
    writer
        .write_all(br#"<?xml version="1.0" encoding="UTF-8"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:fldChar w:fldCharType="begin"/><w:instrText xml:space="preserve"> PAGE </w:instrText><w:fldChar w:fldCharType="end"/></w:r></w:p></w:ftr>"#)
        .map_err(|error| error.to_string())?;
    writer
        .finish()
        .map_err(|error| format!("完成 DOCX 写入失败: {error}"))?;
    Ok(buffer)
}

fn document_to_docx_xml(document: &Value) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>"#,
    );
    if let Some(pages) = document.get("pages").and_then(Value::as_array) {
        for (page_index, page) in pages.iter().enumerate() {
            if page_index > 0 {
                xml.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
            }
            let mut flow = DocxFlowParagraph::default();
            for block in page
                .get("blocks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let content = block
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if content.is_empty() {
                    continue;
                }
                let block_type = block
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("paragraph");
                let bbox = converter_block_bbox(block);
                let display = block.get("displayOrInline").and_then(Value::as_str);
                let inline_formula = block_type == "formula" && display == Some("inline");
                let plain_text = matches!(block_type, "text" | "paragraph");
                let can_join = if flow.runs.is_empty() {
                    true
                } else if inline_formula || flow.last_was_formula {
                    same_line_geometry(flow.last_bbox, bbox)
                } else {
                    plain_text && adjacent_text_geometry(flow.last_bbox, bbox, flow.paragraph_left)
                };
                if (plain_text || inline_formula) && can_join {
                    if inline_formula {
                        flow.push_formula(block, bbox);
                    } else {
                        flow.push_text(content, bbox);
                    }
                    continue;
                }
                flow.flush_into(&mut xml);
                if plain_text || inline_formula {
                    if inline_formula {
                        flow.push_formula(block, bbox);
                    } else {
                        flow.push_text(content, bbox);
                    }
                } else {
                    push_standalone_docx_block(&mut xml, block, block_type, content);
                }
            }
            flow.flush_into(&mut xml);
        }
    }
    xml.push_str(r#"<w:sectPr><w:headerReference w:type="default" r:id="rIdHeader"/><w:footerReference w:type="default" r:id="rIdFooter"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720"/></w:sectPr></w:body></w:document>"#);
    xml
}

#[derive(Default)]
struct DocxFlowParagraph {
    runs: String,
    last_bbox: Option<[f64; 4]>,
    paragraph_left: Option<f64>,
    last_was_formula: bool,
}

impl DocxFlowParagraph {
    fn push_text(&mut self, content: &str, bbox: Option<[f64; 4]>) {
        let needs_space = !self.runs.is_empty();
        self.runs.push_str("<w:r><w:t xml:space=\"preserve\">");
        if needs_space {
            self.runs.push(' ');
        }
        self.runs.push_str(&escape_xml(content));
        self.runs.push_str("</w:t></w:r>");
        if self.paragraph_left.is_none() {
            self.paragraph_left = bbox.map(|value| value[0]);
        }
        self.last_bbox = bbox;
        self.last_was_formula = false;
    }

    fn push_formula(&mut self, block: &Value, bbox: Option<[f64; 4]>) {
        let content = block.get("content").and_then(Value::as_str).unwrap_or("");
        let latex = crate::document_math::latex_from_block(block)
            .or_else(|| {
                block
                    .get("latex")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| normalize_formula(content));
        if !self.runs.is_empty() {
            self.runs
                .push_str("<w:r><w:t xml:space=\"preserve\"> </w:t></w:r>");
        }
        let ast = crate::document_math::ast_from_block(block);
        self.runs
            .push_str(&formula_to_omml(ast.as_ref(), &latex, true));
        if self.paragraph_left.is_none() {
            self.paragraph_left = bbox.map(|value| value[0]);
        }
        self.last_bbox = bbox;
        self.last_was_formula = true;
    }

    fn flush_into(&mut self, xml: &mut String) {
        if self.runs.is_empty() {
            return;
        }
        xml.push_str("<w:p>");
        xml.push_str(&self.runs);
        xml.push_str("</w:p>");
        *self = Self::default();
    }
}

fn push_standalone_docx_block(xml: &mut String, block: &Value, block_type: &str, content: &str) {
    if block_type == "table" {
        xml.push_str(&table_to_docx_xml(content));
        return;
    }
    let level = block
        .get("level")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 6) as u8);
    let style = if block_type == "heading" {
        format!(
            "<w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>",
            level.unwrap_or(2)
        )
    } else if block_type == "list" {
        "<w:pPr><w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>".to_string()
    } else if block_type == "caption" {
        "<w:pPr><w:pStyle w:val=\"Caption\"/></w:pPr>".to_string()
    } else if matches!(block_type, "formula" | "matrix") {
        "<w:pPr><w:pStyle w:val=\"Formula\"/></w:pPr>".to_string()
    } else {
        String::new()
    };
    xml.push_str("<w:p>");
    xml.push_str(&style);
    if matches!(block_type, "formula" | "matrix") {
        let latex = crate::document_math::latex_from_block(block)
            .or_else(|| {
                block
                    .get("latex")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| normalize_formula(content));
        let ast = crate::document_math::ast_from_block(block);
        xml.push_str(&formula_to_omml(ast.as_ref(), &latex, false));
    } else {
        xml.push_str("<w:r><w:t xml:space=\"preserve\">");
        xml.push_str(&escape_xml(content));
        xml.push_str("</w:t></w:r>");
    }
    xml.push_str("</w:p>");
}

fn table_to_docx_xml(content: &str) -> String {
    let rows = content
        .lines()
        .filter(|line| line.contains('|'))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return format!("<w:p><w:r><w:t>{}</w:t></w:r></w:p>", escape_xml(content));
    }
    let mut xml =
        String::from(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr><w:tblGrid>"#);
    let columns = rows.first().map(|row| row.split('|').count()).unwrap_or(1);
    for _ in 0..columns {
        xml.push_str(r#"<w:gridCol w:w="1800"/>"#);
    }
    xml.push_str("</w:tblGrid>");
    for row in rows {
        xml.push_str("<w:tr>");
        for cell in row.split('|') {
            xml.push_str("<w:tc><w:p><w:r><w:t>");
            xml.push_str(&escape_xml(cell.trim()));
            xml.push_str("</w:t></w:r></w:p></w:tc>");
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    xml
}

fn formula_to_omml(
    ast: Option<&crate::document_math::MathNode>,
    value: &str,
    inline: bool,
) -> String {
    let expression = ast
        .map(math_node_to_omml)
        .unwrap_or_else(|| formula_expression_to_omml(value));
    if inline {
        format!(r#"<m:oMath>{expression}</m:oMath>"#)
    } else {
        format!(r#"<m:oMathPara><m:oMath>{expression}</m:oMath></m:oMathPara>"#)
    }
}

fn math_node_to_omml(node: &crate::document_math::MathNode) -> String {
    use crate::document_math::MathNode;
    match node {
        MathNode::Symbol { value }
        | MathNode::Identifier { value }
        | MathNode::Number { value }
        | MathNode::Operator { value } => format!(
            "<m:r><m:t xml:space=\"preserve\">{}</m:t></m:r>",
            escape_xml(value)
        ),
        MathNode::Sequence { children } => children.iter().map(math_node_to_omml).collect(),
        MathNode::Superscript { base, exponent } => format!(
            "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
            math_node_to_omml(base),
            math_node_to_omml(exponent)
        ),
        MathNode::Subscript { base, subscript } => format!(
            "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
            math_node_to_omml(base),
            math_node_to_omml(subscript)
        ),
        MathNode::SubSuperscript {
            base,
            subscript,
            exponent,
        } => format!(
            "<m:sSubSup><m:e>{}</m:e><m:sub>{}</m:sub><m:sup>{}</m:sup></m:sSubSup>",
            math_node_to_omml(base),
            math_node_to_omml(subscript),
            math_node_to_omml(exponent)
        ),
        MathNode::Fraction {
            numerator,
            denominator,
        } => format!(
            "<m:f><m:fPr/><m:num>{}</m:num><m:den>{}</m:den></m:f>",
            math_node_to_omml(numerator),
            math_node_to_omml(denominator)
        ),
        MathNode::Root { radicand } => format!(
            "<m:rad><m:radPr/><m:deg/><m:e>{}</m:e></m:rad>",
            math_node_to_omml(radicand)
        ),
        MathNode::Matrix { rows, delimiter } => {
            let (begin, end) = match delimiter.as_str() {
                "pmatrix" => ("(", ")"),
                "vmatrix" => ("|", "|"),
                _ => ("[", "]"),
            };
            let rows = rows
                .iter()
                .map(|row| {
                    format!(
                        "<m:mr>{}</m:mr>",
                        row.iter()
                            .map(|cell| format!("<m:e>{}</m:e>", math_node_to_omml(cell)))
                            .collect::<String>()
                    )
                })
                .collect::<String>();
            format!("<m:d><m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr><m:e><m:m>{}</m:m></m:e></m:d>", begin, end, rows)
        }
        MathNode::Equation { left, right } => format!(
            "{}<m:r><m:t>=</m:t></m:r>{}",
            math_node_to_omml(left),
            math_node_to_omml(right)
        ),
        MathNode::EquationArray { equations } => format!(
            "<m:eqArr>{}</m:eqArr>",
            equations
                .iter()
                .map(|equation| format!("<m:e>{}</m:e>", math_node_to_omml(equation)))
                .collect::<String>()
        ),
        MathNode::Group { child } => format!(
            "<m:d><m:dPr><m:begChr m:val=\"(\"/><m:endChr m:val=\")\"/></m:dPr><m:e>{}</m:e></m:d>",
            math_node_to_omml(child)
        ),
    }
}

/// Emit a small deterministic OMML subset for the structures most commonly
/// recovered from a PDF text layer. Unknown LaTeX is retained as escaped text
/// in a normal math run; it is never interpreted by guessing.
fn formula_expression_to_omml(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(rows) = parse_latex_matrix(trimmed) {
        let mut xml = String::from(
            "<m:d><m:dPr><m:begChr m:val=\"[\"/><m:endChr m:val=\"]\"/></m:dPr><m:e><m:m>",
        );
        for row in rows {
            xml.push_str("<m:mr>");
            for cell in row {
                xml.push_str("<m:e>");
                xml.push_str(&formula_expression_to_omml(cell));
                xml.push_str("</m:e>");
            }
            xml.push_str("</m:mr>");
        }
        xml.push_str("</m:m></m:e></m:d>");
        return xml;
    }
    if let Some(radicand) = trimmed
        .strip_prefix(r"\sqrt{")
        .and_then(|value| value.strip_suffix('}'))
    {
        return format!(
            "<m:rad><m:radPr/><m:deg/><m:e>{}</m:e></m:rad>",
            formula_expression_to_omml(radicand)
        );
    }
    if let Some((numerator, denominator)) = parse_frac(trimmed) {
        return format!(
            r#"<m:f><m:fPr/><m:num>{}</m:num><m:den>{}</m:den></m:f>"#,
            formula_expression_to_omml(numerator),
            formula_expression_to_omml(denominator)
        );
    }
    let chars = trimmed.chars().collect::<Vec<_>>();
    let mut result = String::new();
    let mut plain = String::new();
    let mut index = 0usize;
    let flush_plain = |result: &mut String, plain: &mut String| {
        if !plain.is_empty() {
            result.push_str("<m:r><m:t xml:space=\"preserve\">");
            result.push_str(&escape_xml(plain));
            result.push_str("</m:t></m:r>");
            plain.clear();
        }
    };
    while index < chars.len() {
        if matches!(chars[index], '^' | '_') {
            let marker = chars[index];
            let Some(base) = plain.pop() else {
                plain.push(marker);
                index += 1;
                continue;
            };
            let (argument, next_index) = formula_argument(&chars, index + 1);
            if argument.is_empty() {
                plain.push(base);
                plain.push(marker);
                index += 1;
                continue;
            }
            flush_plain(&mut result, &mut plain);
            let base_xml = format!("<m:r><m:t>{}</m:t></m:r>", escape_xml(&base.to_string()));
            let argument_xml = formula_expression_to_omml(&argument);
            if marker == '^' {
                result.push_str(&format!(
                    "<m:sSup><m:e>{base_xml}</m:e><m:sup>{argument_xml}</m:sup></m:sSup>"
                ));
            } else {
                result.push_str(&format!(
                    "<m:sSub><m:e>{base_xml}</m:e><m:sub>{argument_xml}</m:sub></m:sSub>"
                ));
            }
            index = next_index;
            continue;
        }
        plain.push(chars[index]);
        index += 1;
    }
    flush_plain(&mut result, &mut plain);
    result
}

fn parse_latex_matrix(value: &str) -> Option<Vec<Vec<&str>>> {
    let body = value
        .strip_prefix(r"\begin{bmatrix}")?
        .strip_suffix(r"\end{bmatrix}")?;
    let rows = body
        .split(r"\\")
        .map(|row| row.split('&').map(str::trim).collect::<Vec<_>>())
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>();
    let columns = rows.first()?.len();
    (columns > 0 && rows.iter().all(|row| row.len() == columns)).then_some(rows)
}

fn formula_argument(chars: &[char], start: usize) -> (String, usize) {
    if chars.get(start) == Some(&'{') {
        let mut depth = 1usize;
        let mut index = start + 1;
        let mut value = String::new();
        while index < chars.len() {
            match chars[index] {
                '{' => {
                    depth += 1;
                    value.push('{');
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return (value, index + 1);
                    }
                    value.push('}');
                }
                character => value.push(character),
            }
            index += 1;
        }
        (String::new(), start)
    } else {
        chars
            .get(start)
            .map(|character| (character.to_string(), start + 1))
            .unwrap_or_default()
    }
}

fn parse_frac(value: &str) -> Option<(&str, &str)> {
    let remainder = value.strip_prefix(r"\frac{")?;
    let (numerator, next) = brace_group(remainder)?;
    let remainder = remainder.get(next..)?.strip_prefix('{')?;
    let (denominator, end) = brace_group(remainder)?;
    (end == remainder.len()).then_some((numerator, denominator))
}

fn brace_group(value: &str) -> Option<(&str, usize)> {
    let mut depth = 1usize;
    for (offset, character) in value.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((&value[..offset], offset + character.len_utf8()));
                }
            }
            _ => {}
        }
    }
    None
}

fn validate_xml(value: &str) -> Result<(), String> {
    let mut reader = quick_xml::Reader::from_str(value);
    reader.config_mut().trim_text(false);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => return Ok(()),
            Ok(_) => {}
            Err(error) => return Err(format!("生成的 XML 无法解析: {error}")),
        }
    }
}

fn document_to_pdf(document: &Value) -> Vec<u8> {
    let lines = document_to_text(document)
        .lines()
        .flat_map(|line| wrap_ascii(line, 90))
        .collect::<Vec<_>>();
    let mut content = String::from("BT\n/F1 11 Tf\n50 760 Td\n");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            content.push_str("0 -15 Td\n");
        }
        content.push('(');
        content.push_str(&escape_pdf(line));
        content.push_str(") Tj\n");
    }
    content.push_str("ET\n");
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        page,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            content
        ),
    ];
    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize);
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn document_to_pdf_with_source(document: &Value) -> Vec<u8> {
    let source_path = document["source"]["path"].as_str().unwrap_or("");
    if Path::new(source_path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        if let Ok(bytes) = std::fs::read(source_path) {
            // A native PDF source already contains the authoritative page
            // geometry, fonts and image resources.  Preserve it for a PDF
            // target instead of degrading it to a single Helvetica page.
            return bytes;
        }
    }
    document_to_pdf(document)
}

fn wrap_ascii(value: &str, width: usize) -> Vec<String> {
    let ascii = value
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect::<String>();
    if ascii.is_empty() {
        return vec![String::new()];
    }
    ascii
        .as_bytes()
        .chunks(width)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}

fn escape_pdf(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn escape_xml(value: &str) -> String {
    escape_html(value).replace('\'', "&apos;")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Cursor, Read};

    fn document() -> Value {
        json!({
            "id": "doc-1",
            "source": { "path": "C:/input.txt" },
            "pages": [{ "number": 1, "blocks": [
                { "id": "b1", "type": "heading", "content": "标题" },
                { "id": "b2", "type": "paragraph", "content": "正文 & 内容" }
            ]}]
        })
    }

    #[test]
    fn converts_text_formats() {
        assert!(document_to_text(&document()).contains("正文"));
        assert!(document_to_markdown(&document()).starts_with("##"));
        assert!(document_to_html(&document()).contains("&amp;"));
    }

    #[test]
    fn joins_pdf_text_segments_without_crossing_formula_or_column_boundaries() {
        let document = json!({"pages":[{"number":1,"blocks":[
            {"type":"text","content":"This is a","bbox":[40.0,700.0,90.0,712.0]},
            {"type":"text","content":"line of text.","bbox":[94.0,700.0,155.0,712.0]},
            {"type":"text","content":"It contin-","bbox":[40.0,684.0,90.0,696.0]},
            {"type":"text","content":"ues here.","bbox":[40.0,668.0,88.0,680.0]},
            {"type":"formula","content":"Ax=b","bbox":[100.0,668.0,130.0,680.0]},
            {"type":"text","content":"right column","bbox":[350.0,668.0,430.0,680.0]}
        ]}]});
        let blocks = document_blocks(&document);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].2, "This is a line of text. It continues here.");
        assert_eq!(blocks[1].1, "formula");
        assert_eq!(blocks[2].2, "right column");
    }

    #[test]
    fn markdown_distinguishes_inline_and_display_math() {
        let document = json!({"pages":[{"number":1,"blocks":[
            {"type":"formula","content":"x_1","latex":"x_{1}","displayOrInline":"inline"},
            {"type":"matrix","content":"matrix","latex":"\\begin{bmatrix}1&0\\\\0&1\\end{bmatrix}","displayOrInline":"display"}
        ]}]});
        let markdown = document_to_markdown(&document);
        assert!(markdown.contains("$x_{1}$"));
        assert!(markdown.contains("$$\n\\begin{bmatrix}"));
    }

    #[test]
    fn markdown_keeps_inline_formula_inside_surrounding_sentence() {
        let document = json!({"pages":[{"number":1,"blocks":[
            {"type":"paragraph","content":"Solve"},
            {"type":"formula","content":"Ax=b","latex":"Ax=b","displayOrInline":"inline"},
            {"type":"paragraph","content":"for x."}
        ]}]});
        assert_eq!(document_to_markdown(&document), "Solve $Ax=b$ for x.\n\n");
    }

    #[test]
    fn output_directory_builds_a_file_name_and_never_overwrites_same_format_source() {
        assert_eq!(
            output_path_in_directory("C:/input/report.pdf", "docx", "D:/exports").unwrap(),
            Path::new("D:/exports").join("report.docx")
        );
        assert_eq!(
            output_path_in_directory("C:/input/report.pdf", "pdf", "D:/exports").unwrap(),
            Path::new("D:/exports").join("report-converted.pdf")
        );
    }

    #[test]
    fn emits_docx_and_pdf_signatures() {
        let docx = document_to_docx(&document()).unwrap();
        assert_eq!(&docx[..2], b"PK");
        assert!(document_to_pdf(&document()).starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn generated_docx_parts_are_well_formed_and_structured() {
        let document = json!({
            "id": "doc-quality",
            "source": { "path": "C:/input.pdf" },
            "pages": [{ "number": 1, "blocks": [
                { "id": "h1", "type": "heading", "level": 1, "content": "Chapter 1" },
                { "id": "p1", "type": "paragraph", "content": "determi\u{0002}nants" },
                { "id": "f1", "type": "formula", "latex": "A^T A x = A^T b", "content": "A^T A x = A^T b" },
                { "id": "m1", "type": "matrix", "latex": "\\begin{bmatrix}1 & 1 \\\\ 2 & 3 \\\\ 3 & 4\\end{bmatrix}", "content": "matrix" },
                { "id": "l1", "type": "list", "content": "first item" },
                { "id": "t1", "type": "table", "content": "a|b\nc|d" }
            ]}]
        });
        let bytes = document_to_docx(&document).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut document_xml = String::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            if entry.name().ends_with(".xml") || entry.name().ends_with(".rels") {
                let mut xml = String::new();
                entry.read_to_string(&mut xml).unwrap();
                validate_xml(&xml)
                    .unwrap_or_else(|error| panic!("{} is invalid: {error}", entry.name()));
                if entry.name() == "word/document.xml" {
                    document_xml = xml;
                }
            }
        }
        assert!(document_xml.contains("Heading1"));
        assert!(document_xml.contains("m:oMath"));
        assert!(document_xml.contains("m:sSup"));
        assert!(document_xml.contains("<m:m>"));
        assert_eq!(document_xml.matches("<m:mr>").count(), 3);
        assert!(document_xml.contains("w:numPr"));
        assert!(document_xml.contains("w:tbl"));
        assert!(!document_xml.chars().any(|character| {
            let code = character as u32;
            (code < 0x20 && !matches!(character, '\n' | '\r' | '\t')) || code == 0x7f
        }));
    }

    #[test]
    fn docx_prefers_math_ast_and_emits_editable_subsup_and_matrix() {
        let matrix = crate::document_math::MathNode::Matrix {
            rows: vec![
                vec![
                    crate::document_math::MathNode::Number { value: "1".into() },
                    crate::document_math::MathNode::Number { value: "1".into() },
                ],
                vec![
                    crate::document_math::MathNode::Number { value: "2".into() },
                    crate::document_math::MathNode::Number { value: "3".into() },
                ],
            ],
            delimiter: "pmatrix".into(),
        };
        let subsup = crate::document_math::MathNode::SubSuperscript {
            base: Box::new(crate::document_math::MathNode::Identifier { value: "x".into() }),
            subscript: Box::new(crate::document_math::MathNode::Number { value: "1".into() }),
            exponent: Box::new(crate::document_math::MathNode::Number { value: "2".into() }),
        };
        let document = json!({"pages":[{"number":1,"blocks":[
            {"type":"formula","content":"wrong fallback","math":{"ast":subsup}},
            {"type":"matrix","content":"wrong fallback","math":{"ast":matrix}}
        ]}]});
        let xml = document_to_docx_xml(&document);
        assert!(xml.contains("<m:sSubSup>"));
        assert!(xml.contains("<m:begChr m:val=\"(\"/>"));
        assert_eq!(xml.matches("<m:mr>").count(), 2);
        assert_eq!(
            xml.matches("<m:e><m:r><m:t xml:space=\"preserve\">1")
                .count(),
            2
        );
        assert!(!xml.contains("wrong fallback"));
    }

    #[test]
    fn inline_formula_uses_omath_without_omath_para() {
        let ast = crate::document_math::MathNode::Equation {
            left: Box::new(crate::document_math::MathNode::Identifier { value: "Ax".into() }),
            right: Box::new(crate::document_math::MathNode::Identifier { value: "b".into() }),
        };
        let document = json!({"pages":[{"number":1,"blocks":[{
            "type":"formula",
            "content":"Ax=b",
            "displayOrInline":"inline",
            "math":{"ast":ast}
        }]}]});
        let xml = document_to_docx_xml(&document);
        assert!(xml.contains("<m:oMath>"));
        assert!(!xml.contains("<m:oMathPara>"));
    }

    #[test]
    fn docx_keeps_surrounding_text_and_inline_formula_in_one_paragraph() {
        let document = json!({"pages":[{"number":1,"blocks":[
            {"type":"text","content":"Solve","bbox":[40.0,700.0,75.0,712.0]},
            {"type":"formula","content":"Ax=b","latex":"Ax=b","displayOrInline":"inline","bbox":[80.0,700.0,110.0,712.0]},
            {"type":"text","content":"for x.","bbox":[115.0,700.0,150.0,712.0]}
        ]}]});
        let xml = document_to_docx_xml(&document);
        assert_eq!(xml.matches("<w:p>").count(), 1);
        assert!(xml.contains("Solve"));
        assert!(xml.contains("<m:oMath>"));
        assert!(xml.contains("for x."));
    }
}
