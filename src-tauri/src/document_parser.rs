//! Native parsers for the non-OCR part of Document Engine.
//!
//! The parser deliberately produces the same envelope as `pdf_parser`: a
//! route plus a unified `Document` object.  PDF stays delegated to the PDF
//! route so its per-page OCR requirements are preserved.  Office formats are
//! read directly from their ZIP/XML package without extracting user files to
//! disk.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_XML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
struct ParsedPage {
    number: usize,
    text: String,
}

/// Parse one supported local document into the unified Document JSON shape.
pub fn parse_file(path: &str) -> Result<Value, String> {
    let file_path = Path::new(path);
    let metadata =
        std::fs::metadata(file_path).map_err(|error| format!("无法访问文档: {error}"))?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(format!(
            "文档超过 {} MiB 大小限制",
            MAX_DOCUMENT_BYTES / 1024 / 1024
        ));
    }
    let bytes = std::fs::read(file_path).map_err(|error| format!("读取文档失败: {error}"))?;
    let extension = file_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "pdf" {
        return crate::pdf_parser::parse_file(path);
    }

    let pages = match extension.as_str() {
        "txt" | "text" | "md" | "markdown" => {
            let text = decode_text(&bytes)?;
            vec![ParsedPage { number: 1, text }]
        }
        "html" | "htm" => vec![ParsedPage {
            number: 1,
            text: html_to_text(&decode_text(&bytes)?),
        }],
        "docx" => parse_docx(&bytes)?,
        "pptx" => parse_pptx(&bytes)?,
        "xlsx" => parse_xlsx(&bytes)?,
        _ => return Err(format!("不支持的文档格式: .{extension}")),
    };
    let kind = match extension.as_str() {
        "txt" | "text" => "text",
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "docx" => "docx",
        "pptx" => "pptx",
        "xlsx" => "xlsx",
        _ => "unknown",
    };
    Ok(build_result(path, &bytes, &extension, kind, pages))
}

fn build_result(
    path: &str,
    bytes: &[u8],
    extension: &str,
    kind: &str,
    pages: Vec<ParsedPage>,
) -> Value {
    let source_hash = {
        let mut digest = Sha256::new();
        digest.update(bytes);
        format!("{:x}", digest.finalize())
    };
    let mut page_json = Vec::with_capacity(pages.len());
    let mut reading_order = Vec::new();
    let mut outline = Vec::new();
    let mut has_tables = false;
    let mut has_formulas = false;
    let mut title: Option<String> = None;
    for page in pages {
        let mut blocks = Vec::new();
        let mut paragraph_index = 0usize;
        for paragraph in split_paragraphs(&page.text) {
            paragraph_index += 1;
            let (mut block_type, mut content, level) = markdown_heading(&paragraph);
            if block_type == "paragraph" && is_formula_block(&content) {
                block_type = "formula";
                content = strip_formula_delimiters(&content);
                has_formulas = true;
            } else if block_type == "paragraph" && (content.contains('|') || content.contains('\t'))
            {
                block_type = "table";
                has_tables = true;
            }
            let id = format!("p{}-b{paragraph_index}", page.number);
            reading_order.push(id.clone());
            let mut block = json!({
                "id": id,
                "type": block_type,
                "content": content,
                "rawText": paragraph.clone(),
                "language": detect_language(&paragraph),
            });
            if block_type == "formula" {
                block["latex"] = json!(
                    crate::formula_ocr::recognize_text(
                        block["content"].as_str().unwrap_or_default()
                    )
                    .latex
                );
            }
            if let Some(level) = level {
                block["level"] = json!(level);
                title.get_or_insert_with(|| {
                    block["content"].as_str().unwrap_or_default().to_string()
                });
                outline.push(json!({
                    "id": format!("p{}-b{paragraph_index}", page.number),
                    "title": content,
                    "level": level,
                    "page": page.number,
                    "children": []
                }));
            }
            blocks.push(block);
        }
        page_json.push(json!({
            "number": page.number,
            "width": 0,
            "height": 0,
            "blocks": blocks,
        }));
    }
    let page_count = page_json.len();
    let mime = match extension {
        "txt" | "text" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    };
    let document = json!({
        "id": format!("doc-{source_hash}"),
        "source": {
            "path": path,
            "mime": mime,
            "size": bytes.len(),
            "hash": source_hash,
            "engine": "native",
            "engineVersion": format!("builtin-{kind}-1")
        },
        "metadata": {
            "pageCount": page_count,
            "hasTextLayer": true,
            "isScanned": false,
            "hasTables": has_tables,
            "hasFormulas": has_formulas || (kind == "xlsx" && bytes.windows(2).any(|window| window == b"f>")),
            "title": title,
            "hasImages": false,
            "encoding": if matches!(kind, "text" | "markdown" | "html") { Some(detect_encoding(bytes).to_string()) } else { None::<String> }
        },
        "pages": page_json,
        "structure": {
            "outline": outline,
            "readingOrder": reading_order
        }
    });
    json!({
        "route": "native",
        "requiresOcr": false,
        "ocrPageNumbers": [],
        "warnings": [],
        "document": document
    })
}

fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) {
        let utf16 = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
        return String::from_utf16(&utf16.collect::<Vec<_>>())
            .map_err(|error| format!("UTF-16LE 文档解码失败: {error}"));
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        let utf16 = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
        return String::from_utf16(&utf16.collect::<Vec<_>>())
            .map_err(|error| format!("UTF-16BE 文档解码失败: {error}"));
    }
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn detect_encoding(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        "utf-8-bom"
    } else if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        "utf-16"
    } else if std::str::from_utf8(bytes).is_ok() {
        "utf-8"
    } else {
        "binary-or-unsupported"
    }
}

fn split_paragraphs(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .split("\n\n")
        .flat_map(|paragraph| {
            let trimmed = paragraph.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![trimmed.to_string()]
            }
        })
        .collect()
}

fn markdown_heading(text: &str) -> (&'static str, String, Option<u8>) {
    let trimmed = text.trim();
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        return (
            "heading",
            trimmed[hashes + 1..].trim().to_string(),
            Some(hashes as u8),
        );
    }
    ("paragraph", trimmed.to_string(), None)
}

fn is_formula_block(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with("$$") && trimmed.ends_with("$$"))
        || (trimmed.starts_with("\\[") && trimmed.ends_with("\\]"))
        || (trimmed.starts_with("\\(") && trimmed.ends_with("\\)"))
}

fn strip_formula_delimiters(text: &str) -> String {
    let trimmed = text.trim();
    for (open, close) in [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)")] {
        if trimmed.starts_with(open)
            && trimmed.ends_with(close)
            && trimmed.len() >= open.len() + close.len()
        {
            return trimmed[open.len()..trimmed.len() - close.len()]
                .trim()
                .to_string();
        }
    }
    trimmed.to_string()
}

fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0usize;
    let bytes = html.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor] == b'<' {
            if let Some(end) = bytes[cursor..].iter().position(|byte| *byte == b'>') {
                let tag = &html[cursor + 1..cursor + end];
                let name = tag
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "p" | "div" | "br" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "tr"
                ) {
                    output.push('\n');
                }
                cursor += end + 1;
                continue;
            }
        }
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'<' {
            cursor += 1;
        }
        output.push_str(&decode_entities(&html[start..cursor]));
    }
    output
}

fn parse_docx(bytes: &[u8]) -> Result<Vec<ParsedPage>, String> {
    let xml = read_zip_entry(bytes, "word/document.xml")?;
    let paragraphs = xml_paragraphs(&xml, "w:p");
    if paragraphs.is_empty() {
        return Err("DOCX 中未找到可解析的段落".into());
    }
    Ok(vec![ParsedPage {
        number: 1,
        text: paragraphs.join("\n\n"),
    }])
}

fn parse_pptx(bytes: &[u8]) -> Result<Vec<ParsedPage>, String> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("打开 PPTX 包失败: {error}"))?;
    let mut names = (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|file| file.name().to_string())
        })
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    names.sort_by_key(|name| {
        name.trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    let mut pages = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let xml = read_zip_entry(bytes, name)?;
        let paragraphs = xml_paragraphs(&xml, "a:p");
        pages.push(ParsedPage {
            number: index + 1,
            text: paragraphs.join("\n\n"),
        });
    }
    if pages.is_empty() {
        return Err("PPTX 中未找到幻灯片".into());
    }
    Ok(pages)
}

fn parse_xlsx(bytes: &[u8]) -> Result<Vec<ParsedPage>, String> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("打开 XLSX 包失败: {error}"))?;
    let mut names = (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|file| file.name().to_string())
        })
        .filter(|name| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    names.sort();
    let shared_strings = read_zip_entry_optional(bytes, "xl/sharedStrings.xml")
        .ok()
        .flatten()
        .map(|xml| xml_paragraphs(&xml, "si").join("\n"))
        .unwrap_or_default();
    let mut pages = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let xml = read_zip_entry(bytes, name)?;
        let mut text = xml_to_text(&xml, "v");
        if text.is_empty() {
            text = xml_to_text(&xml, "t");
        }
        if !shared_strings.is_empty() {
            text = format!("{shared_strings}\n{text}");
        }
        pages.push(ParsedPage {
            number: index + 1,
            text,
        });
    }
    if pages.is_empty() {
        return Err("XLSX 中未找到工作表".into());
    }
    Ok(pages)
}

fn read_zip_entry(bytes: &[u8], name: &str) -> Result<String, String> {
    read_zip_entry_optional(bytes, name)?.ok_or_else(|| format!("ZIP 包缺少 {name}"))
}

fn read_zip_entry_optional(bytes: &[u8], name: &str) -> Result<Option<String>, String> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("打开 Office 包失败: {error}"))?;
    let Ok(mut file) = archive.by_name(name) else {
        return Ok(None);
    };
    if file.size() > MAX_XML_ENTRY_BYTES {
        return Err(format!("Office XML 条目过大: {name}"));
    }
    let mut content = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut content)
        .map_err(|error| format!("读取 Office XML 失败: {error}"))?;
    Ok(Some(String::from_utf8_lossy(&content).into_owned()))
}

fn xml_paragraphs(xml: &str, tag: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = 0usize;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    while let Some(relative) = xml[cursor..].find(&open) {
        let start = cursor + relative;
        let Some(open_end_relative) = xml[start..].find('>') else {
            break;
        };
        let content_start = start + open_end_relative + 1;
        let Some(close_relative) = xml[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + close_relative;
        let text = xml_to_text(&xml[content_start..content_end], "w:t");
        let text = if text.is_empty() {
            xml_to_text(&xml[content_start..content_end], "a:t")
        } else {
            text
        };
        if !text.trim().is_empty() {
            result.push(text.trim().to_string());
        }
        cursor = content_end + close.len();
    }
    result
}

fn xml_to_text(xml: &str, tag: &str) -> String {
    let mut output = String::new();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut cursor = 0usize;
    while let Some(relative) = xml[cursor..].find(&open) {
        let start = cursor + relative;
        let Some(open_end_relative) = xml[start..].find('>') else {
            break;
        };
        let content_start = start + open_end_relative + 1;
        let Some(close_relative) = xml[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + close_relative;
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&decode_entities(xml[content_start..content_end].trim()));
        cursor = content_end + close.len();
    }
    output
}

fn decode_entities(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0usize;
    while let Some(relative) = value[cursor..].find('&') {
        let start = cursor + relative;
        output.push_str(&value[cursor..start]);
        let Some(end_relative) = value[start..].find(';') else {
            output.push_str(&value[start..]);
            return output;
        };
        let end = start + end_relative;
        let entity = &value[start + 1..end];
        let replacement = match entity {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" => "'".to_string(),
            _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|character| character.to_string())
                .unwrap_or_else(|| format!("&{entity};")),
            _ if entity.starts_with('#') => entity[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|character| character.to_string())
                .unwrap_or_else(|| format!("&{entity};")),
            _ => format!("&{entity};"),
        };
        output.push_str(&replacement);
        cursor = end + 1;
    }
    output.push_str(&value[cursor..]);
    output
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, content: &[u8]) -> String {
        let dir = std::env::temp_dir().join(format!(
            "cb-document-parser-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn parses_markdown_into_heading_and_paragraph() {
        let path = temp_file("readme.md", "# Title\n\n正文内容\n".as_bytes());
        let output = parse_file(&path).unwrap();
        assert_eq!(output["route"], "native");
        assert_eq!(
            output["document"]["pages"][0]["blocks"][0]["type"],
            "heading"
        );
        assert_eq!(
            output["document"]["structure"]["outline"][0]["title"],
            "Title"
        );
    }

    #[test]
    fn parses_html_entities_and_tags() {
        let path = temp_file("page.html", b"<h1>Hello</h1><p>A &amp; B</p>");
        let output = parse_file(&path).unwrap();
        assert_eq!(
            output["document"]["pages"][0]["blocks"][0]["content"],
            "Hello"
        );
        assert!(output["document"]["pages"][0]["blocks"][1]["content"]
            .as_str()
            .unwrap()
            .contains("A & B"));
    }

    #[test]
    fn parses_docx_xml_without_extracting_to_disk() {
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive
            .write_all(
                b"<w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:body></w:document>",
            )
            .unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        let path = temp_file("doc.docx", &bytes);
        let output = parse_file(&path).unwrap();
        assert_eq!(
            output["document"]["pages"][0]["blocks"][0]["content"],
            "Hello"
        );
    }

    #[test]
    fn rejects_unknown_format() {
        let path = temp_file("data.bin", b"data");
        assert!(parse_file(&path).is_err());
    }
}
