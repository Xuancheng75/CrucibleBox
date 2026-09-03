//! Layout-stage policy shared by native PDF parsing and OCR enrichment.
//!
//! This module intentionally does not recognize formulas.  It only provides
//! conservative candidate filtering and a geometry-based fallback for pages
//! where the optional layout model is unavailable.  A formula recognizer may
//! only receive a block after this policy (or a real layout detector) has
//! classified its region as `formula`.

#![allow(dead_code)]

/// Minimal layout input shared by formula detectors and recognizers. The
/// rendered image remains owned by the page pipeline; this contract keeps
/// model adapters independent of a particular image/runtime backend.
#[derive(Debug, Clone)]
pub struct PageImage {
    pub page: usize,
    pub width: u32,
    pub height: u32,
    pub path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FormulaRegion {
    pub page: usize,
    pub bbox: Option<[f32; 4]>,
    pub text: String,
    pub display: bool,
}

/// Layout detection is a page-semantic operation. Text acquisition is not a
/// substitute for layout detection, so this trait is separate from OCR.
pub trait LayoutDetector: Send + Sync {
    fn detect(&self, page: &PageImage) -> Result<LayoutResult, String>;
}

pub trait FormulaDetector: Send + Sync {
    fn detect(&self, page: &PageImage) -> Result<Vec<FormulaRegion>, String>;
}

#[derive(Debug, Clone, Default)]
pub struct LayoutResult {
    pub regions: Vec<LayoutRegion>,
}

#[derive(Debug, Clone)]
pub struct LayoutRegion {
    pub kind: RegionKind,
    pub bbox: [f32; 4],
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Text,
    Heading,
    Header,
    Footer,
    PageNumber,
    Formula,
}

impl RegionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Heading => "heading",
            Self::Header => "header",
            Self::Footer => "footer",
            Self::PageNumber => "page_number",
            Self::Formula => "formula",
        }
    }

    pub const fn excluded_from_rag(self) -> bool {
        matches!(self, Self::Header | Self::Footer | Self::PageNumber)
    }
}

/// Reject strings that are known non-formula document content before any
/// formula recognizer is called.
pub fn is_strict_formula_candidate(value: &str) -> bool {
    let text = value.trim();
    if text.is_empty() || text.chars().count() > 180 {
        return false;
    }
    if is_forbidden_document_token(text) {
        return false;
    }

    let operators = text
        .chars()
        .filter(|character| {
            matches!(
                character,
                '=' | '＝'
                    | '+'
                    | '−'
                    | '-'
                    | '×'
                    | '÷'
                    | '^'
                    | '_'
                    | '√'
                    | '∑'
                    | '∫'
                    | '≤'
                    | '≥'
                    | '≠'
                    | '∞'
                    | '∂'
                    | '∈'
                    | '±'
            )
        })
        .count();
    let has_math_symbol = text.chars().any(|character| {
        "αβγδεζηθλμνξπρστφχω∑∫√≤≥≠∞∂∈±".contains(character)
            || matches!(character, '^' | '_' | '≤' | '≥' | '≠')
    });
    let has_digit = text.chars().any(|character| character.is_ascii_digit());
    let words = text.split_whitespace().collect::<Vec<_>>();
    let long_words = words
        .iter()
        .filter(|word| {
            word.chars()
                .filter(|character| character.is_alphabetic())
                .count()
                >= 3
        })
        .count();
    let compact_len = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();

    // A slash-separated heading such as "TIDE / WIND / MEMORY" is prose,
    // not math.  Keep slash formulas possible only when they also contain an
    // equality, a digit, a Greek/math symbol, or single-letter operands.
    let slash_only = text.contains('/')
        && !text
            .chars()
            .any(|character| matches!(character, '=' | '＝' | '^' | '_' | '∑' | '∫' | '√'))
        && !has_digit
        && !has_math_symbol;
    if slash_only && (long_words >= 2 || words.len() > 4) {
        return false;
    }

    // Sentences and ordinary headings may contain a hyphen or a slash.  A
    // formula candidate must remain compact and math-dense.
    if words.len() > 8 || long_words > 3 {
        return false;
    }
    operators > 0
        && compact_len <= 180
        && (has_math_symbol || has_digit || text.contains('=') || text.contains('＝'))
}

fn is_forbidden_document_token(text: &str) -> bool {
    let compact = text.replace(' ', "");
    let is_time = compact.len() == 5
        && compact.chars().enumerate().all(|(index, character)| {
            if index == 2 {
                character == ':'
            } else {
                character.is_ascii_digit()
            }
        });
    let is_page_number = compact.split('/').count() == 2
        && compact.split('/').all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        });
    let is_date = compact.matches(['-', '/']).count() >= 2
        && compact
            .chars()
            .filter(|character| character.is_ascii_digit())
            .count()
            >= 6;
    let is_url_or_path =
        text.contains("://") || text.contains("\\") || (text.contains('/') && text.contains('.'));
    let is_isbn_or_identifier = compact.to_ascii_lowercase().starts_with("isbn")
        || compact
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .count()
            >= 10
            && compact
                .chars()
                .filter(|character| !character.is_ascii_alphanumeric())
                .all(|character| matches!(character, '-' | '_' | '.' | '/'));
    is_time || is_page_number || is_date || is_url_or_path || is_isbn_or_identifier
}

/// Geometry-based fallback used while the real PP-DocLayout model is being
/// loaded or when it is intentionally disabled.  It does not replace model
/// output; it only prevents obvious headers, footers and visual headings from
/// being treated as ordinary text.
pub fn classify_fallback(content: &str, bbox: Option<[f32; 4]>, page_height: f32) -> RegionKind {
    let text = content.trim();
    let Some([x1, y1, x2, y2]) = bbox else {
        return if is_strict_formula_candidate(text) {
            RegionKind::Formula
        } else {
            RegionKind::Text
        };
    };
    let height = (y2 - y1).max(0.0);
    let width = (x2 - x1).max(0.0);
    let top_ratio = y1 / page_height.max(1.0);
    let bottom_ratio = y2 / page_height.max(1.0);
    let short = text.chars().count() <= 80;
    if bottom_ratio >= 0.94 && short {
        return if text.chars().all(|character| character.is_ascii_digit())
            || is_page_number_text(text)
        {
            RegionKind::PageNumber
        } else {
            RegionKind::Footer
        };
    }
    if top_ratio <= 0.06 && short {
        return RegionKind::Header;
    }
    if short && height >= 1.35 * estimated_body_height(width, text) {
        return RegionKind::Heading;
    }
    if is_strict_formula_candidate(text) {
        RegionKind::Formula
    } else {
        RegionKind::Text
    }
}

fn is_page_number_text(value: &str) -> bool {
    let compact = value.replace(' ', "");
    compact.split('/').count() == 2
        && compact.split('/').all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

fn estimated_body_height(width: f32, text: &str) -> f32 {
    // A stable geometry proxy, not a font-size guess: short narrow lines are
    // more likely to be headings when their block is visibly taller.
    (width / text.chars().count().max(1) as f32 * 0.08).clamp(6.0, 24.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_headers_pages_times_and_dates() {
        for value in [
            "TIDE / WIND / MEMORY",
            "FIELD ARCHIVE / FOGHARBOR",
            "03/10",
            "08:05",
            "2025-03-10",
            "https://example.com/a.pdf",
        ] {
            assert!(!is_strict_formula_candidate(value), "{value}");
        }
    }

    #[test]
    fn accepts_compact_math_expression() {
        for value in ["Ax = 0", "A^T A x = A^T b", "λ1 + λ2 = 1", "A^{-1}"] {
            assert!(is_strict_formula_candidate(value), "{value}");
        }
    }
}
