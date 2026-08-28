//! PDF parser and PDFium renderer used by Document Engine.
//!
//! Text-layer extraction stays dependency-light and deterministic. Scanned
//! pages are rendered by the packaged PDFium runtime and then handed to the
//! Rust OCR worker by the trusted service.

use flate2::read::ZlibDecoder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OBJECTS: usize = 100_000;
const PDF_RENDER_WIDTH: i32 = 1800;

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
    let config = pdfium_bundled::pdfium_render::prelude::PdfRenderConfig::new()
        .set_target_width(PDF_RENDER_WIDTH)
        .set_maximum_height(2400);
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

fn bind_pdfium() -> Result<pdfium_bundled::pdfium_render::prelude::Pdfium, String> {
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
    for candidate in candidates {
        if candidate.is_file() {
            return pdfium_bundled::bind_pdfium_from_path(&candidate)
                .map_err(|error| format!("绑定 PDFium 失败 ({}): {error}", candidate.display()));
        }
    }
    pdfium_bundled::bind_pdfium_silent()
        .map_err(|error| format!("PDFium 不可用（请随包提供 pdfium.dll）: {error}"))
}

/// Report whether the packaged/system PDFium runtime can be located without
/// downloading anything. Binding is deferred until a scan page is requested.
pub fn renderer_status() -> Value {
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
    let path = candidates.into_iter().find(|candidate| candidate.is_file());
    json!({
        "available": path.is_some() || pdfium_bundled::is_pdfium_cached(),
        "path": path.map(|value| value.to_string_lossy().into_owned())
            .or_else(|| pdfium_bundled::cached_pdfium_path().map(|value| value.to_string_lossy().into_owned())),
        "version": pdfium_bundled::PDFIUM_VERSION,
        "runtimeDownload": true
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

    // Some generated PDFs omit a conventional page dictionary while still
    // containing `/Type /Page` in raw bytes. Keep the result useful and
    // deterministic instead of silently returning zero pages.
    if page_objects.is_empty() {
        let fallback_count = count_page_markers(bytes).max(1);
        return Err(format!(
            "PDF page tree is unsupported (detected {fallback_count} page marker(s))"
        ));
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
                blocks.push(json!({
                    "id": id,
                    "type": "text",
                    "content": content,
                    "language": detect_language(content),
                }));
            }
        }
        // Table/formula classification is deliberately conservative. A later
        // layout parser may enrich these blocks without changing this schema.
        has_tables |= page_text.contains('|');
        has_formulas |= page_text.contains("\\(") || page_text.contains("\\[");
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
                        let mut value = byte - b'0';
                        for _ in 0..2 {
                            if let Some(next) = bytes.get(cursor + 1) {
                                if (b'0'..=b'7').contains(next) {
                                    value = value * 8 + *next - b'0';
                                    cursor += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                        decoded.push(value);
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
            for pair in nibbles.chunks_exact(2) {
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
        && bytes.chunks_exact(2).take(8).any(|pair| pair[0] == 0);
    if (bytes.starts_with(&[0xfe, 0xff]) && bytes.len() >= 2) || utf16_without_bom {
        let mut result = String::new();
        let payload = if bytes.starts_with(&[0xfe, 0xff]) {
            &bytes[2..]
        } else {
            bytes
        };
        for pair in payload.chunks_exact(2) {
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

    #[cfg(windows)]
    #[test]
    fn pdfium_renders_embedded_scan_page() {
        let image = image::load_from_memory(include_bytes!("../../ocr-worker/test.png"))
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
        let _ = std::fs::remove_file(output);
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
    fn rejects_non_pdf() {
        let path = write_pdf(b"not a pdf");
        assert!(parse_file(&path).is_err());
    }
}
