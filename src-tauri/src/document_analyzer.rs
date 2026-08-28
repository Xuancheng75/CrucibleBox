// Document Analyzer（Phase 3）
// 纯 Rust 文件结构检测：MIME/扩展名/大小/页数/文本层/图片/表格/公式启发式，
// 并据此给出自动路由决策（native / paddleocr / mineru）。
//
// 设计边界（诚实标注，不伪造）：
// - 扩展名 + magic bytes 检测为可靠事实。
// - PDF 页数 / 文本层为**启发式字节扫描**（统计 /Type /Page 与 /Font + 文本算子），
//   非完整 PDF 解析；精确检测（含表格/公式/阅读顺序）需 MinerU（Python）落地。
// - 表格/公式/复杂布局的精确识别在后续 Phase（MinerU Worker）补全。

use crate::db::Db;
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Pdf,
    Image,
    Office,
    Text,
    Unknown,
}

fn category_from_extension(ext: &str) -> Category {
    match ext.to_ascii_lowercase().as_str() {
        "pdf" => Category::Pdf,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" => Category::Image,
        "docx" | "pptx" | "xlsx" => Category::Office,
        "md" | "markdown" | "html" | "htm" | "txt" | "text" => Category::Text,
        _ => Category::Unknown,
    }
}

/// magic bytes → 精确 MIME（比扩展名更可信）
fn mime_from_magic(bytes: &[u8], ext: &str) -> String {
    if bytes.starts_with(b"%PDF") {
        return "application/pdf".into();
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "image/png".into();
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return "image/jpeg".into();
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return "image/webp".into();
    }
    if bytes.starts_with(b"BM") {
        return "image/bmp".into();
    }
    if bytes.starts_with(b"II*\x00") || bytes.starts_with(b"MM\x00*") {
        return "image/tiff".into();
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return match ext.to_ascii_lowercase().as_str() {
            "docx" => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into()
            }
            "pptx" => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation".into()
            }
            "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
            _ => "application/zip".into(),
        };
    }
    if matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown") {
        return "text/markdown".into();
    }
    if matches!(ext.to_ascii_lowercase().as_str(), "html" | "htm") {
        return "text/html".into();
    }
    if matches!(ext.to_ascii_lowercase().as_str(), "txt" | "text") {
        return "text/plain".into();
    }
    "application/octet-stream".into()
}

/// 扫描 PDF 原始字节：统计页面数与是否含文本层（启发式）
fn analyze_pdf(bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);

    // 页面数：统计 "/Type" 后紧跟 "/Page"（排除 "/Pages" 父节点）
    let page_count = count_pdf_pages(&text);

    // 文本层：存在字体资源 (/Font) 且存在文本显示算子 (BT..ET / Tj / TJ)
    let has_font = text.contains("/Font");
    let has_text_operator = text.contains(" BT")
        || text.contains("/BT")
        || text.contains("Tj")
        || text.contains("TJ")
        || text.contains("ET");
    let has_text_layer = has_font && has_text_operator;

    // 图片对象（XObject Image）
    let has_images = text.contains("/Subtype /Image") || text.contains("/Subtype/Image");

    // 表格/公式：启发式（按需增强）；当前仅标记无法可靠判定
    let has_tables = false; // 需 MinerU 布局分析
    let has_formulas = false; // 需 MinerU 公式检测

    let is_scanned = !has_text_layer;
    let recommended_engine = if has_text_layer {
        // 有文本层：优先 native parser；复杂布局后续可升级 MinerU
        "native"
    } else {
        // 扫描件：MinerU 负责布局/OCR，PaddleOCR 兜底
        "mineru"
    };

    json!({
        "pageCount": page_count,
        "hasTextLayer": has_text_layer,
        "isScanned": is_scanned,
        "hasImages": has_images,
        "hasTables": has_tables,
        "hasFormulas": has_formulas,
        "recommendedEngine": recommended_engine,
        "textLayerDetection": "heuristic",
    })
}

/// 统计 /Type /Page 出现次数（排除父节点 /Pages）
fn count_pdf_pages(text: &str) -> u32 {
    let mut count = 0u32;
    let mut idx = 0;
    while let Some(pos) = text[idx..].find("/Type") {
        let abs = idx + pos;
        let rest = &text[abs + 5..];
        let after = rest.trim_start_matches([' ', '\n', '\r']);
        if let Some(tail) = after.strip_prefix("/Page") {
            // /Pages 为父节点（"/Page" 后紧跟 's' 且 s 后接 token 边界），不计为页
            let is_pages_parent = tail.starts_with('s')
                && (tail.len() == 1
                    || tail[1..].starts_with(|c: char| {
                        c.is_whitespace() || matches!(c, '>' | '/' | ']' | '<' | '(')
                    }));
            if !is_pages_parent {
                count += 1;
            }
        }
        idx = abs + 5;
    }
    count
}

/// 文件编码检测（文本类）：BOM + UTF-8 有效性
fn detect_encoding(bytes: &[u8]) -> String {
    if bytes.starts_with(b"\xef\xbb\xbf") {
        return "utf-8-bom".into();
    }
    if bytes.starts_with(b"\xff\xfe") || bytes.starts_with(b"\xfe\xff") {
        return "utf-16".into();
    }
    if std::str::from_utf8(bytes).is_ok() {
        "utf-8".into()
    } else {
        "binary-or-unsupported".into()
    }
}

/// 入口：分析单个文件，返回统一 DocumentAnalysis JSON
pub fn analyze_file(_db: &Db, _plugin_id: &str, path: &str) -> Value {
    let p = Path::new(path);
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
        return err_analysis("invalid-path", format!("无法解析文件路径: {path}"));
    };
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();

    let Ok(meta) = std::fs::metadata(p) else {
        return err_analysis("file-not-found", format!("文件不存在或无法访问: {path}"));
    };
    let size = meta.len();

    let Ok(bytes) = std::fs::read(p) else {
        return err_analysis("read-failed", format!("无法读取文件: {path}"));
    };

    let mime = mime_from_magic(&bytes, &ext);
    let category = category_from_extension(&ext);

    // source hash：用于缓存 key
    let hash = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&bytes);
        format!("{:x}", h.finalize())
    };

    let (support_level, recommended_engine, detail) = match category {
        Category::Pdf => {
            let pdf = analyze_pdf(&bytes);
            let engine = pdf["recommendedEngine"]
                .as_str()
                .unwrap_or("native")
                .to_string();
            ("supported".to_string(), engine, pdf)
        }
        Category::Image => ("supported".to_string(), "paddleocr".to_string(), json!({})),
        Category::Office => (
            "experimental".to_string(),
            "native".to_string(),
            json!({ "note": "DOCX/XLSX/PPTX 解析待 Phase 8 Converter 实现" }),
        ),
        Category::Text => (
            "supported".to_string(),
            "native".to_string(),
            json!({ "encoding": detect_encoding(&bytes) }),
        ),
        Category::Unknown => (
            "unsupported".to_string(),
            "none".to_string(),
            json!({ "note": "不支持的文件类型" }),
        ),
    };

    json!({
        "path": path,
        "fileName": name,
        "mime": mime,
        "extension": ext,
        "size": size,
        "hash": hash,
        "category": format!("{:?}", category).to_ascii_lowercase(),
        "supportLevel": support_level,
        "recommendedEngine": recommended_engine,
        "detail": detail,
    })
}

fn err_analysis(code: &str, message: String) -> Value {
    json!({ "error": message, "code": code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_db() -> Db {
        let dir = std::env::temp_dir().join(format!(
            "cb-analyzer-db-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Db::open(&dir.join("openbox.db")).unwrap()
    }

    fn write_temp(name: &str, bytes: &[u8]) -> String {
        let dir = std::env::temp_dir().join(format!(
            "cb-analyzer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        drop(f);
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn pdf_with_text_layer_routes_native() {
        let pdf = b"%PDF-1.4\n1 0 obj<</Type/Page>>endobj\n2 0 obj<</Font>>endobj\nBT /F1 12 Tf (Hello) Tj ET\n%%EOF";
        let path = write_temp("doc.pdf", pdf);
        let db = make_db();
        let out = analyze_file(&db, "document-engine", &path);
        assert_eq!(out["category"], "pdf");
        assert_eq!(out["detail"]["hasTextLayer"], true);
        assert_eq!(out["recommendedEngine"], "native");
        assert_eq!(out["hash"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn pdf_without_text_routes_mineru() {
        let pdf = b"%PDF-1.4\n1 0 obj<</Type/Page>>endobj\n2 0 obj<</Subtype/Image>>endobj\n%%EOF";
        let path = write_temp("scan.pdf", pdf);
        let db = make_db();
        let out = analyze_file(&db, "document-engine", &path);
        assert_eq!(out["detail"]["hasTextLayer"], false);
        assert_eq!(out["detail"]["isScanned"], true);
        assert_eq!(out["recommendedEngine"], "mineru");
    }

    #[test]
    fn image_routes_paddleocr() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        let path = write_temp("img.png", png);
        let db = make_db();
        let out = analyze_file(&db, "document-engine", &path);
        assert_eq!(out["category"], "image");
        assert_eq!(out["mime"], "image/png");
        assert_eq!(out["recommendedEngine"], "paddleocr");
    }

    #[test]
    fn missing_file_reports_error() {
        let db = make_db();
        let out = analyze_file(&db, "document-engine", "C:\\no\\such\\file.pdf");
        assert_eq!(out["code"], "file-not-found");
    }

    #[test]
    fn unknown_extension_unsupported() {
        let path = write_temp("weird.xyz", b"data");
        let db = make_db();
        let out = analyze_file(&db, "document-engine", &path);
        assert_eq!(out["supportLevel"], "unsupported");
    }
}
