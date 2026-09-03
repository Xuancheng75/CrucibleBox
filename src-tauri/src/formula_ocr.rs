//! Formula detection/recognition contracts for Document Engine.
//!
//! Detection is owned by the layout pipeline.  This module only recognizes a
//! region that an upstream detector has already classified as formula.  The
//! current text adapter is deliberately deterministic and lightweight; a
//! native PP-FormulaNet or RapidLaTeXOCR implementation can replace it
//! without changing Document IR or the exporters.

#![allow(dead_code)]

use crate::document_layout::FormulaRegion;

pub const ADAPTER_ENGINE: &str = "formula-ocr-adapter-v1";
pub const ADAPTER_MODEL_VERSION: &str = "text-normalization-v1";

/// Stable result contract shared by native and fallback recognizers.
#[derive(Debug, Clone)]
pub struct FormulaRecognitionResult {
    pub raw_latex: String,
    pub normalized_latex: String,
    pub plain_text: String,
    pub confidence: f32,
    pub engine: String,
    pub model_version: String,
    pub bbox: Option<[f32; 4]>,
    pub display_or_inline: &'static str,
}

/// A recognizer must receive an already classified formula region.  It must
/// not inspect arbitrary paragraph text and decide that it is mathematical.
pub trait FormulaRecognizer: Send + Sync {
    fn recognize(&self, region: &FormulaRegion) -> Result<FormulaRecognitionResult, String>;
}

/// Compatibility result retained for the existing text-only call sites.

#[derive(Debug, Clone)]
pub struct FormulaResult {
    pub latex: String,
    pub raw_latex: String,
    pub normalized_latex: String,
    pub plain_text: String,
    pub confidence: f32,
    pub engine: &'static str,
    pub model_version: &'static str,
    pub display_or_inline: &'static str,
}

pub fn recognize_text(value: &str) -> FormulaResult {
    let result = recognize_text_result(value, None, true);
    FormulaResult {
        latex: result.normalized_latex.clone(),
        raw_latex: result.raw_latex,
        normalized_latex: result.normalized_latex,
        plain_text: result.plain_text,
        confidence: result.confidence,
        engine: ADAPTER_ENGINE,
        model_version: ADAPTER_MODEL_VERSION,
        display_or_inline: result.display_or_inline,
    }
}

/// Recognize an already accepted formula region with the current fallback
/// adapter.  `display` is supplied by the layout detector and is not guessed
/// from arbitrary document prose.
pub fn recognize_region(region: &FormulaRegion) -> FormulaRecognitionResult {
    recognize_text_result(&region.text, region.bbox, region.display)
}

struct TextFormulaRecognizer;

impl FormulaRecognizer for TextFormulaRecognizer {
    fn recognize(&self, region: &FormulaRegion) -> Result<FormulaRecognitionResult, String> {
        Ok(recognize_region(region))
    }
}

pub fn text_adapter() -> impl FormulaRecognizer {
    TextFormulaRecognizer
}

fn recognize_text_result(
    value: &str,
    bbox: Option<[f32; 4]>,
    display: bool,
) -> FormulaRecognitionResult {
    let raw_latex = value.trim().to_string();
    let normalized_latex = normalize(value);
    let symbol_count = value
        .chars()
        .filter(|character| {
            matches!(
                character,
                '=' | '＋'
                    | '+'
                    | '-'
                    | '−'
                    | '×'
                    | '÷'
                    | '/'
                    | '^'
                    | '√'
                    | '∑'
                    | '∫'
                    | '≤'
                    | '≥'
                    | '≠'
                    | '∞'
                    | '∂'
                    | '∈'
                    | '∉'
                    | '≈'
                    | '±'
            )
        })
        .count();
    let confidence = if normalized_latex.is_empty() {
        0.0
    } else if symbol_count > 0 {
        0.82
    } else {
        0.60
    };
    FormulaRecognitionResult {
        raw_latex,
        normalized_latex,
        plain_text: value.trim().to_string(),
        confidence,
        engine: ADAPTER_ENGINE.to_string(),
        model_version: ADAPTER_MODEL_VERSION.to_string(),
        bbox,
        display_or_inline: if display { "display" } else { "inline" },
    }
}

fn normalize(value: &str) -> String {
    let value = expand_unicode_math(value);
    let mut output = String::with_capacity(value.len() + 8);
    let mut chars = value.trim().chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '＝' => output.push('='),
            '＋' => output.push('+'),
            '−' => output.push('-'),
            '×' => output.push_str(r"\times "),
            '÷' => output.push_str(r"\div "),
            '≤' => output.push_str(r"\leq "),
            '≥' => output.push_str(r"\geq "),
            '≠' => output.push_str(r"\neq "),
            '∞' => output.push_str(r"\infty "),
            '∑' => output.push_str(r"\sum "),
            '∫' => output.push_str(r"\int "),
            '√' => output.push_str(r"\sqrt{"),
            '^' => {
                output.push('^');
                if let Some(next) = chars.peek().copied() {
                    if next != '{' && !next.is_whitespace() {
                        output.push('{');
                        output.push(next);
                        chars.next();
                        output.push('}');
                    }
                }
            }
            '_' => {
                output.push('_');
                if let Some(next) = chars.peek().copied() {
                    if next != '{' && !next.is_whitespace() {
                        output.push('{');
                        output.push(next);
                        chars.next();
                        output.push('}');
                    }
                }
            }
            character if character.is_whitespace() => {
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            character => output.push(character),
        }
    }
    let normalized = collapse_adjacent_duplicate_tokens(output.trim());
    let mut normalized = normalized;
    if normalized.contains(r"\sqrt{") && !normalized.ends_with('}') {
        normalized.push('}');
    }
    normalized
}

/// PDF text layers can repeat an extracted glyph sequence when a visible
/// symbol is represented by overlapping text objects.  Collapse only exact
/// adjacent whitespace-delimited duplicates; this is intentionally not a
/// spelling or formula inference heuristic.
fn collapse_adjacent_duplicate_tokens(value: &str) -> String {
    let mut result = Vec::new();
    for token in value.split_whitespace() {
        if result.last().is_some_and(|previous| *previous == token) {
            continue;
        }
        result.push(token);
    }
    result.join(" ")
}

/// PDF text extraction often returns superscript/subscript glyphs as separate
/// Unicode characters. Convert the common mathematical ranges into the same
/// brace form used by the formula adapter so Markdown and OMML receive a
/// stable structured expression rather than a character fragment stream.
fn expand_unicode_math(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        let mapped = match character {
            '⁰' => Some("^0"),
            '¹' => Some("^1"),
            '²' => Some("^2"),
            '³' => Some("^3"),
            '⁴' => Some("^4"),
            '⁵' => Some("^5"),
            '⁶' => Some("^6"),
            '⁷' => Some("^7"),
            '⁸' => Some("^8"),
            '⁹' => Some("^9"),
            '⁺' => Some("^+"),
            '⁻' => Some("^-"),
            '⁽' => Some("^("),
            '⁾' => Some("^)"),
            '₀' => Some("_0"),
            '₁' => Some("_1"),
            '₂' => Some("_2"),
            '₃' => Some("_3"),
            '₄' => Some("_4"),
            '₅' => Some("_5"),
            '₆' => Some("_6"),
            '₇' => Some("_7"),
            '₈' => Some("_8"),
            '₉' => Some("_9"),
            _ => None,
        };
        if let Some(mapped) = mapped {
            result.push_str(mapped);
        } else {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_math_symbols_to_latex() {
        let result = recognize_text("x² ＋ y² ＝ z²");
        assert_eq!(result.latex, "x^{2} + y^{2} = z^{2}");
        assert!(result.confidence > 0.8);
    }

    #[test]
    fn wraps_simple_superscript() {
        let result = recognize_text("x^2 + y_1");
        assert_eq!(result.latex, "x^{2} + y_{1}");
    }

    #[test]
    fn collapses_only_adjacent_duplicate_formula_tokens() {
        let result = recognize_text("2x 2x + 3y");
        assert_eq!(result.normalized_latex, "2x + 3y");
        let separated = recognize_text("x + x");
        assert_eq!(separated.normalized_latex, "x + x");
    }
}
