//! Machine-readable quality counters for Document IR and renderer gates.

use serde_json::{json, Value};

/// Evaluate the quality of native text without changing the source payload.
/// `rawText` remains available for diagnostics; normalized `content` is the
/// only text consumed by RAG and renderers.
pub fn annotate_native_text_quality(document: &mut Value) -> Value {
    let mut good_pages = 0usize;
    let mut suspicious_pages = 0usize;
    let mut unusable_pages = 0usize;
    let mut needs_ocr_pages = 0usize;
    let mut native_blocks = 0usize;
    let mut ocr_blocks = 0usize;
    if let Some(pages) = document.get_mut("pages").and_then(Value::as_array_mut) {
        for page in pages {
            let Some(blocks) = page.get_mut("blocks").and_then(Value::as_array_mut) else {
                continue;
            };
            let mut page_score_sum = 0.0_f32;
            let mut page_score_count = 0usize;
            let mut page_native_count = 0usize;
            for block in blocks.iter_mut() {
                let is_native = block["source"]
                    .as_str()
                    .is_some_and(|source| source.starts_with("native"));
                match block["source"].as_str() {
                    Some(_) if is_native => {
                        native_blocks += 1;
                        page_native_count += 1;
                    }
                    Some("ocr") => ocr_blocks += 1,
                    _ => {}
                }
                if !is_native {
                    continue;
                }
                let raw = block["rawText"]
                    .as_str()
                    .or_else(|| block["content"].as_str())
                    .unwrap_or_default();
                let content = block["content"].as_str().unwrap_or_default();
                let raw_len = raw.chars().count().max(1) as f32;
                let controls = crate::document_text::count_invalid_xml_chars(raw);
                let replacement = raw.matches('\u{fffd}').count();
                // PDF font encodings often place recoverable separators
                // between otherwise correct glyphs.  Those controls are
                // retained in rawText for diagnostics but have already been
                // removed/repaired in content, so they must not make a good
                // native page fall back to OCR or disappear from RAG.
                let content_empty = content.trim().is_empty();
                let score = (1.0
                    - (replacement as f32 / raw_len).min(0.75)
                    - if content_empty { 0.65 } else { 0.0 })
                .clamp(0.0, 1.0);
                let status = if score >= 0.82 {
                    "good"
                } else if score >= 0.45 {
                    "suspicious"
                } else {
                    "unusable"
                };
                block["nativeTextQualityScore"] = json!(score);
                block["nativeTextStatus"] = json!(status);
                block["nativeTextRawControlCharCount"] = json!(controls);
                block["nativeTextReplacementCharCount"] = json!(replacement);
                block["needsOcr"] = json!(status == "unusable");
                if status == "unusable" {
                    block["excludedFromRag"] = json!(true);
                    block["qualityReason"] = json!("corrupted_native_text");
                }
                page_score_sum += score;
                page_score_count += 1;
            }
            if page_native_count == 0 {
                continue;
            }
            let page_score = if page_score_count == 0 {
                0.0
            } else {
                page_score_sum / page_score_count as f32
            };
            let page_status = if page_score >= 0.82 {
                good_pages += 1;
                "good"
            } else if page_score >= 0.45 {
                suspicious_pages += 1;
                "suspicious"
            } else {
                unusable_pages += 1;
                "unusable"
            };
            if page_status == "unusable" {
                needs_ocr_pages += 1;
            }
            page["nativeTextQualityScore"] = json!(page_score);
            page["nativeTextStatus"] = json!(page_status);
            page["needsOcr"] = json!(page_status != "good");
        }
    }
    json!({
        "nativeTextGoodPages": good_pages,
        "nativeTextSuspiciousPages": suspicious_pages,
        "nativeTextUnusablePages": unusable_pages,
        "nativeTextNeedsOcrPages": needs_ocr_pages,
        "nativeTextBlockCount": native_blocks,
        "ocrTextBlockCount": ocr_blocks
    })
}

pub fn report(document: &Value, removed_control_chars: usize) -> Value {
    let mut heading_count = 0usize;
    let mut suspected_false_heading_count = 0usize;
    let mut toc_entry_count = 0usize;
    let mut formula_block_count = 0usize;
    let mut image_block_count = 0usize;
    let mut table_block_count = 0usize;
    let mut native_text_block_count = 0usize;
    let mut ocr_text_block_count = 0usize;
    let mut figure_block_count = 0usize;
    let mut heading_candidate_count = 0usize;
    let mut accepted_heading_count = 0usize;
    let mut rejected_heading_count = 0usize;
    let mut heading_confidence_sum = 0.0_f64;
    let mut heading_confidence_count = 0usize;
    let mut path_blocks = 0usize;
    let mut eligible_blocks = 0usize;
    let mut orphan_section_count = 0usize;
    let mut chapter_jump_warning_count = 0usize;
    let mut section_ids = std::collections::HashSet::new();
    let mut parent_ids = Vec::new();
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
                            if (block["semanticType"] == "numbered_paragraph"
                                || block["semanticType"] == "exercise_number")
                                && block["headingCandidate"].as_bool().unwrap_or(false) =>
                        {
                            suspected_false_heading_count += 1;
                        }
                        "toc_entry" => toc_entry_count += 1,
                        "formula" => formula_block_count += 1,
                        "image" => image_block_count += 1,
                        "table" => table_block_count += 1,
                        "figure" => figure_block_count += 1,
                        _ => {}
                    }
                    if block["headingCandidate"].as_bool().unwrap_or(false) {
                        heading_candidate_count += 1;
                        if block_type == "heading" {
                            accepted_heading_count += 1;
                        } else {
                            rejected_heading_count += 1;
                        }
                    }
                    if let Some(confidence) = block["headingConfidence"].as_f64() {
                        heading_confidence_sum += confidence;
                        heading_confidence_count += 1;
                    }
                    if !matches!(
                        block["region"].as_str(),
                        Some("toc" | "header" | "footer" | "page_number")
                    ) && !content.trim().is_empty()
                    {
                        eligible_blocks += 1;
                        if block["sectionPath"]
                            .as_str()
                            .is_some_and(|path| !path.is_empty())
                        {
                            path_blocks += 1;
                        }
                    }
                    if let Some(section_id) = block["sectionId"].as_str() {
                        section_ids.insert(section_id.to_string());
                    }
                    if let Some(parent_id) = block["parentId"].as_str() {
                        parent_ids.push(parent_id.to_string());
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
    for parent_id in parent_ids {
        if !section_ids.contains(&parent_id) {
            orphan_section_count += 1;
        }
    }
    let invalid_parent_child_count = orphan_section_count;
    if let Some(outline) = document["structure"]["outline"].as_array() {
        let mut previous_chapter = 0_u64;
        for entry in outline {
            let level = entry["level"].as_u64().unwrap_or(0);
            if level == 1 {
                let number = entry["title"]
                    .as_str()
                    .and_then(|title| {
                        title
                            .split_whitespace()
                            .find_map(|part| part.parse::<u64>().ok())
                    })
                    .unwrap_or(previous_chapter);
                if number > previous_chapter.saturating_add(1) && previous_chapter > 0 {
                    chapter_jump_warning_count += 1;
                }
                previous_chapter = number;
            }
        }
    }
    let native_quality = annotate_native_quality_from_document(document);
    json!({
        "invalidControlChars": 0,
        "invalidXmlChars": invalid_xml_chars,
        "removedInvalidControlChars": removed_control_chars,
        "docxOpenTest": "not_run",
        "docxXmlParse": "not_run",
        "headingCount": heading_count,
        "headingCandidateCount": heading_candidate_count,
        "acceptedHeadingCount": accepted_heading_count,
        "rejectedHeadingCount": rejected_heading_count,
        "suspectedFalseHeadingCount": suspected_false_heading_count + rejected_heading_count,
        "headingConfidenceAverage": if heading_confidence_count == 0 { 0.0 } else { heading_confidence_sum / heading_confidence_count as f64 },
        "tocEntryCount": toc_entry_count,
        "sectionPathAccuracySample": section_samples,
        "formulaBlockCount": formula_block_count,
        "imageBlockCount": image_block_count,
        "tableBlockCount": table_block_count,
        "nativeTextBlockCount": native_text_block_count,
        "ocrTextBlockCount": ocr_text_block_count,
        "figureBlockCount": figure_block_count,
        "sectionPathCoverage": if eligible_blocks == 0 { 1.0 } else { path_blocks as f64 / eligible_blocks as f64 },
        "sectionPathRatio": if eligible_blocks == 0 { 1.0 } else { path_blocks as f64 / eligible_blocks as f64 },
        "treeConsistencyScore": if eligible_blocks == 0 { 1.0 } else { 1.0 - (invalid_parent_child_count as f64 / eligible_blocks as f64).min(1.0) },
        "orphanSectionCount": orphan_section_count,
        "chapterJumpWarningCount": chapter_jump_warning_count,
        "invalidParentChildCount": invalid_parent_child_count,
        "sectionPathConfidence": if eligible_blocks == 0 { 1.0 } else { (path_blocks.saturating_sub(orphan_section_count) as f64 / eligible_blocks as f64).clamp(0.0, 1.0) },
        "nativeTextQuality": native_quality
    })
}

fn annotate_native_quality_from_document(document: &Value) -> Value {
    let mut result = json!({
        "nativeTextGoodPages": 0,
        "nativeTextSuspiciousPages": 0,
        "nativeTextUnusablePages": 0,
        "nativeTextNeedsOcrPages": 0
    });
    if let Some(pages) = document["pages"].as_array() {
        for page in pages {
            let status = page["nativeTextStatus"].as_str().unwrap_or_default();
            let key = match status {
                "good" => "nativeTextGoodPages",
                "suspicious" => "nativeTextSuspiciousPages",
                "unusable" => "nativeTextUnusablePages",
                _ => continue,
            };
            result[key] = json!(result[key].as_u64().unwrap_or(0) + 1);
            if status != "good" {
                result["nativeTextNeedsOcrPages"] =
                    json!(result["nativeTextNeedsOcrPages"].as_u64().unwrap_or(0) + 1);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_quality_marks_corrupted_block_without_losing_raw_text() {
        let mut document = json!({
            "pages": [{ "number": 1, "height": 100, "blocks": [
                { "id": "good", "source": "native/pdf", "content": "determinants", "rawText": "determi\u{2}nants" }
            ] }]
        });
        let stats = annotate_native_text_quality(&mut document);
        assert_eq!(
            document["pages"][0]["blocks"][0]["rawText"],
            "determi\u{2}nants"
        );
        assert_eq!(
            document["pages"][0]["blocks"][0]["nativeTextStatus"],
            "good"
        );
        assert_eq!(document["pages"][0]["blocks"][0]["needsOcr"], false);
        assert_eq!(
            document["pages"][0]["blocks"][0]["nativeTextRawControlCharCount"],
            1
        );
        assert_eq!(stats["nativeTextGoodPages"], 1);
    }
}
