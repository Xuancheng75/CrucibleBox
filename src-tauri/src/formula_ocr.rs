//! Formula OCR adapter for Document Engine.
//!
//! The text worker supplies the glyph sequence for a detected formula region;
//! this adapter turns the sequence into stable display-math LaTeX without
//! loading a second heavyweight model in the common text-only path.  A future
//! native formula model can implement the same result contract and replace
//! `recognize_text` without changing Document IR or converters.

#[derive(Debug, Clone)]
pub struct FormulaResult {
    pub latex: String,
    pub confidence: f32,
    pub engine: &'static str,
}

pub fn recognize_text(value: &str) -> FormulaResult {
    let latex = normalize(value);
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
    let confidence = if latex.is_empty() {
        0.0
    } else if symbol_count > 0 {
        0.82
    } else {
        0.60
    };
    FormulaResult {
        latex,
        confidence,
        engine: "formula-ocr-adapter-v1",
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
    let mut normalized = output.trim().to_string();
    if normalized.contains(r"\sqrt{") && !normalized.ends_with('}') {
        normalized.push('}');
    }
    normalized
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
}
