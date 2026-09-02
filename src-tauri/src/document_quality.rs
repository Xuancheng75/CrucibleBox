//! Machine-readable quality counters for Document IR and renderer gates.

use serde_json::{json, Value};

pub fn report(document: &Value, removed_control_chars: usize) -> Value {
    let mut heading_count = 0usize;
    let mut suspected_false_heading_count = 0usize;
    let mut toc_entry_count = 0usize;
    let mut formula_block_count = 0usize;
    let mut image_block_count = 0usize;
    let mut table_block_count = 0usize;
    let mut native_text_block_count = 0usize;
    let mut ocr_text_block_count = 0usize;
    let mut invalid_xml_chars = 0usize;
    let mut section_samples = Vec::new();

    if let Some(pages) = document["pages"].as_array() {
        for page in pages {
            let page_number = page["number"].as_u64().unwrap_or(1);
            if let Some(blocks) = page["blocks"].as_array() {
                for block in blocks {
                    let block_type = block["type"].as_str().unwrap_or("paragraph");
                    let content = block["content"].as_str().unwrap_or_default();
                    invalid_xml_chars += crate::document_text::count_invalid_xml_chars(content);
                    match block_type {
                        "heading"
                            if matches!(
                                block["semanticType"].as_str(),
                                Some("chapter_heading") | Some("section_heading")
                            ) =>
                        {
                            heading_count += 1;
                            if section_samples.len() < 50 {
                                section_samples.push(json!({
                                    "id": block["id"],
                                    "page": page_number,
                                    "title": content,
                                    "semanticType": block["semanticType"],
                                    "parentId": block["parentId"],
                                    "sectionPath": block["sectionPath"]
                                }));
                            }
                        }
                        "list"
                            if block["semanticType"] == "numbered_paragraph"
                                || block["semanticType"] == "exercise_number" =>
                        {
                            suspected_false_heading_count += 1;
                        }
                        "toc_entry" => toc_entry_count += 1,
                        "formula" => formula_block_count += 1,
                        "image" => image_block_count += 1,
                        "table" => table_block_count += 1,
                        _ => {}
                    }
                    match block["source"].as_str() {
                        Some("native") | Some("native/pdf") => native_text_block_count += 1,
                        Some("ocr") => ocr_text_block_count += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    json!({
        "invalidControlChars": 0,
        "invalidXmlChars": invalid_xml_chars,
        "removedInvalidControlChars": removed_control_chars,
        "docxOpenTest": "not_run",
        "docxXmlParse": "not_run",
        "headingCount": heading_count,
        "suspectedFalseHeadingCount": suspected_false_heading_count,
        "tocEntryCount": toc_entry_count,
        "sectionPathAccuracySample": section_samples,
        "formulaBlockCount": formula_block_count,
        "imageBlockCount": image_block_count,
        "tableBlockCount": table_block_count,
        "nativeTextBlockCount": native_text_block_count,
        "ocrTextBlockCount": ocr_text_block_count
    })
}
