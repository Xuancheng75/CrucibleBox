//! Shared text normalization for every Document IR input and XML exporter.

use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SanitizationStats {
    pub invalid_control_chars_removed: usize,
    pub invalid_xml_chars_removed: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DehyphenationStats {
    pub merged_count: usize,
    pub explicit_hyphen_count: usize,
    pub inferred_split_count: usize,
    pub candidates: Vec<DehyphenationResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DehyphenationResult {
    pub original: String,
    pub merged: String,
    pub confidence: f32,
    pub rule: &'static str,
}

pub fn is_xml_10_char(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{a}' | '\u{d}')
        || matches!(character as u32, 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

/// Normalize to NFC and remove characters forbidden by XML 1.0. Removing a
/// control character without adding whitespace also repairs words split by a
/// corrupt PDF font mapping (for example `determi\u{2}nants`).
pub fn normalize_text(value: &str) -> (String, SanitizationStats) {
    let normalized = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .nfc()
        .collect::<String>();
    let mut output = String::with_capacity(normalized.len());
    let mut stats = SanitizationStats::default();
    for character in normalized.chars() {
        if is_xml_10_char(character) {
            output.push(character);
        } else {
            stats.invalid_xml_chars_removed += 1;
            if character.is_control() {
                stats.invalid_control_chars_removed += 1;
            }
        }
    }
    (output, stats)
}

pub fn count_invalid_xml_chars(value: &str) -> usize {
    value
        .chars()
        .filter(|character| !is_xml_10_char(*character))
        .count()
}

/// Final IR guard. All user-visible textual fields are normalized, including
/// nested table/list children, before the document can be cached or exported.
pub fn sanitize_document(document: &mut Value) -> SanitizationStats {
    fn visit(value: &mut Value, key: Option<&str>, stats: &mut SanitizationStats) {
        match value {
            Value::String(text)
                if matches!(
                    key,
                    Some("content" | "plainText" | "latex" | "title" | "sectionPath" | "caption")
                ) =>
            {
                let (normalized, current) = normalize_text(text);
                *text = normalized;
                stats.invalid_control_chars_removed += current.invalid_control_chars_removed;
                stats.invalid_xml_chars_removed += current.invalid_xml_chars_removed;
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, key, stats);
                }
            }
            Value::Object(object) => {
                for (child_key, child) in object {
                    visit(child, Some(child_key.as_str()), stats);
                }
            }
            _ => {}
        }
    }

    let mut stats = SanitizationStats::default();
    visit(document, None, &mut stats);
    stats
}

/// Repair line-end English word splits only inside blocks that structure
/// recovery has already classified as body paragraphs. Formula, heading,
/// list, code and page-furniture blocks are never passed through this stage.
pub fn repair_body_dehyphenation(document: &mut Value) -> DehyphenationStats {
    let mut stats = DehyphenationStats::default();
    let Some(pages) = document.get_mut("pages").and_then(Value::as_array_mut) else {
        return stats;
    };
    for page in pages {
        let Some(blocks) = page.get_mut("blocks").and_then(Value::as_array_mut) else {
            continue;
        };
        for block in blocks {
            if block["type"] != "paragraph" || block["region"] != "text" {
                continue;
            }
            let Some(content) = block["content"].as_str() else {
                continue;
            };
            let (repaired, mut candidates) = dehyphenate_paragraph(content);
            if candidates.is_empty() {
                continue;
            }
            stats.merged_count += candidates.len();
            stats.explicit_hyphen_count += candidates
                .iter()
                .filter(|candidate| candidate.rule == "explicit_line_hyphen")
                .count();
            stats.inferred_split_count += candidates
                .iter()
                .filter(|candidate| candidate.rule == "known_split_word")
                .count();
            block["content"] = Value::String(repaired);
            block["dehyphenationCount"] = Value::from(candidates.len() as u64);
            block["dehyphenation"] = Value::Array(
                candidates
                    .iter()
                    .map(|candidate| {
                        serde_json::json!({
                            "original": candidate.original,
                            "merged": candidate.merged,
                            "confidence": candidate.confidence,
                            "rule": candidate.rule,
                        })
                    })
                    .collect(),
            );
            stats.candidates.append(&mut candidates);
        }
    }
    stats
}

fn dehyphenate_paragraph(value: &str) -> (String, Vec<DehyphenationResult>) {
    let lines = value.lines().collect::<Vec<_>>();
    if lines.len() < 2 {
        return (value.to_string(), Vec::new());
    }
    let mut output = lines[0].trim_end().to_string();
    let mut results = Vec::new();
    for next_line in lines.iter().skip(1) {
        let next = next_line.trim_start();
        let left = output
            .split_whitespace()
            .next_back()
            .unwrap_or_default()
            .trim_matches(|character: char| !character.is_alphabetic() && character != '-')
            .to_string();
        let right = next
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches(|character: char| !character.is_alphabetic());
        let explicit = left.ends_with('-')
            && left[..left.len().saturating_sub(1)]
                .chars()
                .all(|character| character.is_ascii_alphabetic())
            && right
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_lowercase());
        let known = !explicit && known_split_word(&left, right);
        if explicit || known {
            let left_without_hyphen = left.strip_suffix('-').unwrap_or(&left);
            let merged = format!("{left_without_hyphen}{right}");
            let remove_count = left.chars().count();
            let mut prefix = output.chars().collect::<Vec<_>>();
            prefix.truncate(prefix.len().saturating_sub(remove_count));
            output = prefix.into_iter().collect::<String>();
            output.push_str(&merged);
            output.push_str(next.strip_prefix(right).unwrap_or_default());
            results.push(DehyphenationResult {
                original: format!("{left}\n{right}"),
                merged,
                confidence: if explicit { 0.98 } else { 0.92 },
                rule: if explicit {
                    "explicit_line_hyphen"
                } else {
                    "known_split_word"
                },
            });
        } else {
            output.push('\n');
            output.push_str(next_line);
        }
    }
    (output, results)
}

fn known_split_word(left: &str, right: &str) -> bool {
    if !left
        .chars()
        .all(|character| character.is_ascii_alphabetic())
        || !right
            .chars()
            .all(|character| character.is_ascii_lowercase())
    {
        return false;
    }
    matches!(
        (left.to_ascii_lowercase().as_str(), right),
        ("cer", "tainly")
            | ("dif", "ference")
            | ("determi", "nants")
            | ("matri", "ces")
            | ("triangularmatri", "ces")
            | ("eigen", "values")
            | ("orthog", "onal")
            | ("indepen", "dent")
            | ("transfor", "mation")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removes_xml_controls_and_repairs_split_words() {
        let (value, stats) = normalize_text("determi\u{2}nants\nCer\u{1e}tainly\tOK");
        assert_eq!(value, "determinants\nCertainly\tOK");
        assert_eq!(stats.invalid_control_chars_removed, 2);
        assert_eq!(count_invalid_xml_chars(&value), 0);
    }

    #[test]
    fn recursively_sanitizes_document_text_fields() {
        let mut document = json!({"pages":[{"blocks":[{"content":"A\u{2}B","children":[{"plainText":"C\u{1}D"}]}]}]});
        let stats = sanitize_document(&mut document);
        assert_eq!(document["pages"][0]["blocks"][0]["content"], "AB");
        assert_eq!(stats.invalid_xml_chars_removed, 2);
    }

    #[test]
    fn repairs_only_high_confidence_english_line_splits() {
        let (value, changes) = dehyphenate_paragraph(
            "Cer\ntainly the dif-\nference matters.\nlinear\nalgebra remains separate.",
        );
        assert_eq!(
            value,
            "Certainly the difference matters.\nlinear\nalgebra remains separate."
        );
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn skips_formula_heading_and_list_blocks() {
        let mut document = serde_json::json!({"pages":[{"blocks":[
            {"type":"formula","region":"formula","content":"A\nT"},
            {"type":"heading","region":"heading","content":"Linear\nAlgebra"},
            {"type":"paragraph","region":"text","content":"Dif\nference"}
        ]}]});
        let stats = repair_body_dehyphenation(&mut document);
        assert_eq!(stats.merged_count, 1);
        assert_eq!(document["pages"][0]["blocks"][0]["content"], "A\nT");
        assert_eq!(
            document["pages"][0]["blocks"][1]["content"],
            "Linear\nAlgebra"
        );
        assert_eq!(document["pages"][0]["blocks"][2]["content"], "Difference");
    }
}
