//! Shared text normalization for every Document IR input and XML exporter.

use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SanitizationStats {
    pub invalid_control_chars_removed: usize,
    pub invalid_xml_chars_removed: usize,
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
                    Some(
                        "content"
                            | "rawText"
                            | "plainText"
                            | "latex"
                            | "title"
                            | "sectionPath"
                            | "caption"
                    )
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
}
