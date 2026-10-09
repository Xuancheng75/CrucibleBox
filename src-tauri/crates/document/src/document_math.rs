//! Shared mathematical intermediate representation for every exporter.
//!
//! Detection remains a layout concern. This module only reconstructs blocks
//! that have already been classified as formula/matrix regions.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MathNode {
    Symbol {
        value: String,
    },
    Identifier {
        value: String,
    },
    Number {
        value: String,
    },
    Operator {
        value: String,
    },
    Sequence {
        children: Vec<MathNode>,
    },
    Superscript {
        base: Box<MathNode>,
        exponent: Box<MathNode>,
    },
    Subscript {
        base: Box<MathNode>,
        subscript: Box<MathNode>,
    },
    SubSuperscript {
        base: Box<MathNode>,
        subscript: Box<MathNode>,
        exponent: Box<MathNode>,
    },
    Fraction {
        numerator: Box<MathNode>,
        denominator: Box<MathNode>,
    },
    Root {
        radicand: Box<MathNode>,
    },
    Matrix {
        rows: Vec<Vec<MathNode>>,
        delimiter: String,
    },
    Equation {
        left: Box<MathNode>,
        right: Box<MathNode>,
    },
    EquationArray {
        equations: Vec<MathNode>,
    },
    Group {
        child: Box<MathNode>,
    },
}

pub fn enrich_block(block: &mut Value) {
    if !matches!(block["type"].as_str(), Some("formula" | "matrix")) {
        return;
    }
    let latex = block["normalizedLatex"]
        .as_str()
        .or_else(|| block["latex"].as_str())
        .or_else(|| block["content"].as_str())
        .unwrap_or_default();
    let plain = block["plainText"]
        .as_str()
        .or_else(|| block["content"].as_str())
        .unwrap_or_default();
    let ast = positioned_token_latex(block)
        .map(|reconstructed| parse_expression(&reconstructed))
        .unwrap_or_else(|| parse_math(plain, latex));
    let normalized = to_latex(&ast);
    let (rows, columns, is_matrix) = match &ast {
        MathNode::Matrix { rows, .. } => (
            Some(rows.len()),
            Some(rows.iter().map(Vec::len).max().unwrap_or_default()),
            true,
        ),
        _ => (None, None, false),
    };
    let confidence = block["formulaConfidence"].as_f64().unwrap_or(0.6);
    block["content"] = json!(normalized);
    block["latex"] = json!(normalized);
    block["normalizedLatex"] = json!(normalized);
    block["atomicBlock"] = json!(true);
    block["math"] = json!({
        "ast": ast,
        "rawLatex": block["rawLatex"].clone(),
        "normalizedLatex": block["normalizedLatex"].clone(),
        "confidence": confidence,
        "display": block["displayOrInline"].as_str().unwrap_or("display"),
        "rows": rows,
        "columns": columns,
        "quality": math_quality(confidence),
        "originalTokens": block.get("originalTokens").cloned().unwrap_or(Value::Null),
    });
    block["formulaQuality"] = formula_quality(&ast, confidence);
    if is_matrix {
        block["type"] = json!("matrix");
        block["region"] = json!("formula");
        block["semanticType"] = json!("matrix");
    }
}

fn positioned_token_latex(block: &Value) -> Option<String> {
    let tokens = block.get("originalTokens")?.as_array()?;
    if tokens.len() < 2 {
        return None;
    }
    let mut positioned = tokens
        .iter()
        .filter_map(|token| {
            let text = token.get("originalText")?.as_str()?.trim();
            let bbox = token.get("bbox")?.as_array()?;
            Some((
                bbox.first()?.as_f64()?,
                (bbox.get(1)?.as_f64()? + bbox.get(3)?.as_f64()?) / 2.0,
                (bbox.get(3)?.as_f64()? - bbox.get(1)?.as_f64()?)
                    .abs()
                    .max(1.0),
                text.to_string(),
                token.get("baseline").and_then(Value::as_f64),
                token.get("fontSize").and_then(Value::as_f64),
                token.get("coordinateSystem").and_then(Value::as_str) == Some("pdf"),
            ))
        })
        .collect::<Vec<_>>();
    if positioned.len() < 2 {
        return None;
    }
    positioned.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut heights = positioned
        .iter()
        .map(|token| token.5.unwrap_or(token.2))
        .collect::<Vec<_>>();
    heights.sort_by(f64::total_cmp);
    let body_height = heights.last().copied().unwrap_or(1.0).max(1.0);
    let mut centers = positioned
        .iter()
        .filter(|token| token.5.unwrap_or(token.2) >= body_height * 0.90)
        .map(|token| token.4.unwrap_or(token.1))
        .collect::<Vec<_>>();
    centers.sort_by(f64::total_cmp);
    let baseline_center = centers
        .get(centers.len() / 2)
        .copied()
        .unwrap_or_else(|| positioned[0].4.unwrap_or(positioned[0].1));
    let mut output = String::new();
    let mut spatial_script = false;
    let mut last_script = None;
    let mut index = 0usize;
    while index < positioned.len() {
        let (x, center_y, height, text, baseline, font_size, pdf_coordinates) = &positioned[index];
        let effective_y = baseline.unwrap_or(*center_y);
        let effective_height = font_size.unwrap_or(*height);
        let vertical_delta = if *pdf_coordinates {
            effective_y - baseline_center
        } else {
            baseline_center - effective_y
        };
        if let Some((
            next_x,
            next_center_y,
            next_height,
            next_text,
            next_baseline,
            next_font_size,
            next_pdf_coordinates,
        )) = positioned.get(index + 1)
        {
            let next_effective_y = next_baseline.unwrap_or(*next_center_y);
            let next_effective_height = next_font_size.unwrap_or(*next_height);
            let next_delta = if *next_pdf_coordinates {
                next_effective_y - baseline_center
            } else {
                baseline_center - next_effective_y
            };
            let stacked_pair = effective_height <= body_height * 0.82
                && next_effective_height <= body_height * 0.82
                && (*next_x - *x).abs() <= body_height * 0.55
                && vertical_delta * next_delta < 0.0
                && vertical_delta.abs() > body_height * 0.10
                && next_delta.abs() > body_height * 0.10
                && text.chars().all(char::is_alphanumeric)
                && next_text.chars().all(char::is_alphanumeric);
            if stacked_pair {
                let (numerator, denominator) = if vertical_delta > next_delta {
                    (text, next_text)
                } else {
                    (next_text, text)
                };
                output.push_str("\\frac{");
                output.push_str(numerator);
                output.push_str("}{");
                output.push_str(denominator);
                output.push('}');
                spatial_script = true;
                last_script = None;
                index += 2;
                continue;
            }
        }
        // Font fallback and hinting can vary ordinary glyph heights by around
        // ten percent. Require a materially smaller glyph and a clear vertical
        // displacement before creating script structure.
        let small = effective_height <= body_height * 0.82;
        let script_text = text.chars().all(|character| {
            character.is_alphanumeric() || "αβγδεζηθλμνξπρστφχω∞".contains(character)
        });
        if small && script_text && vertical_delta > body_height * 0.10 && !output.is_empty() {
            if last_script == Some('^') && output.ends_with('}') {
                output.pop();
                output.push_str(text);
                output.push('}');
            } else {
                let sign = output
                    .chars()
                    .last()
                    .filter(|character| matches!(character, '-' | '−'));
                if sign.is_some() {
                    output.pop();
                }
                output.push_str("^{");
                if let Some(sign) = sign {
                    output.push(sign);
                }
                output.push_str(text);
                output.push('}');
            }
            spatial_script = true;
            last_script = Some('^');
        } else if small && script_text && vertical_delta < -body_height * 0.10 && !output.is_empty()
        {
            if last_script == Some('_') && output.ends_with('}') {
                output.pop();
                output.push_str(text);
                output.push('}');
            } else {
                output.push_str("_{");
                output.push_str(text);
                output.push('}');
            }
            spatial_script = true;
            last_script = Some('_');
        } else {
            output.push_str(text);
            last_script = None;
        }
        index += 1;
    }
    spatial_script.then_some(output)
}

pub fn latex_from_block(block: &Value) -> Option<String> {
    let ast = block.get("math")?.get("ast")?.clone();
    serde_json::from_value::<MathNode>(ast)
        .ok()
        .map(|node| to_latex(&node))
}

pub fn ast_from_block(block: &Value) -> Option<MathNode> {
    serde_json::from_value(block.get("math")?.get("ast")?.clone()).ok()
}

/// Produce a deterministic searchable representation without Markdown
/// delimiters or PDF character-level line breaks.
pub fn to_plain_text(node: &MathNode) -> String {
    match node {
        MathNode::Matrix { rows, .. } => format!(
            "[{}]",
            rows.iter()
                .map(|row| format!(
                    "[{}]",
                    row.iter().map(to_plain_text).collect::<Vec<_>>().join(", ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        MathNode::EquationArray { equations } => equations
            .iter()
            .map(to_plain_text)
            .collect::<Vec<_>>()
            .join("; "),
        _ => to_latex(node),
    }
}

pub fn math_quality(confidence: f64) -> &'static str {
    if confidence >= 0.9 {
        "good"
    } else if confidence >= 0.65 {
        "review"
    } else {
        "fallback"
    }
}

pub fn formula_quality(ast: &MathNode, recognition_confidence: f64) -> Value {
    let latex = to_latex(ast);
    let latex_valid = validate_latex(&latex);
    let matrix_valid = matrix_dimensions_valid(ast);
    let suspicious_glyphs = latex
        .chars()
        .filter(|character| {
            let code = *character as u32;
            (0xE000..=0xF8FF).contains(&code)
                || matches!(character, '¤' | '' | '' | '' | '' | '' | '')
        })
        .count();
    let mut score = recognition_confidence.clamp(0.0, 1.0) * 40.0;
    score += if latex_valid { 30.0 } else { 0.0 };
    score += if matrix_valid { 20.0 } else { 0.0 };
    score += if suspicious_glyphs == 0 { 10.0 } else { 0.0 };
    let hard_failure = latex.is_empty() || !latex_valid || !matrix_valid || suspicious_glyphs > 0;
    let level = if hard_failure || score < 65.0 {
        "bad"
    } else if score < 80.0 {
        "review"
    } else if score < 95.0 {
        "good"
    } else {
        "excellent"
    };
    json!({
        "score": score,
        "level": level,
        "astValid": !latex.is_empty() && latex_valid && matrix_valid,
        "latexValid": latex_valid,
        "matrixDimensionsValid": matrix_valid,
        "suspiciousGlyphCount": suspicious_glyphs,
        "recognitionConfidence": recognition_confidence,
    })
}

pub fn validate_latex(value: &str) -> bool {
    if value.is_empty()
        || value.chars().any(|character| {
            let code = character as u32;
            (code < 0x20 && !matches!(character, '\n' | '\r' | '\t'))
                || (0xE000..=0xF8FF).contains(&code)
        })
    {
        return false;
    }
    let mut delimiters = Vec::new();
    for character in value.chars() {
        match character {
            '{' | '(' | '[' => delimiters.push(character),
            '}' | ')' | ']' => {
                let expected = match character {
                    '}' => '{',
                    ')' => '(',
                    ']' => '[',
                    _ => unreachable!(),
                };
                if delimiters.pop() != Some(expected) {
                    return false;
                }
            }
            _ => {}
        }
    }
    delimiters.is_empty()
}

fn matrix_dimensions_valid(node: &MathNode) -> bool {
    match node {
        MathNode::Matrix { rows, .. } => rows.first().is_some_and(|first| {
            !first.is_empty()
                && rows.iter().all(|row| row.len() == first.len())
                && rows.iter().flatten().all(|cell| {
                    let value = to_plain_text(cell);
                    !value.trim().is_empty()
                        && !value
                            .trim_end()
                            .ends_with(|character: char| "+−-*/=^_".contains(character))
                })
        }),
        MathNode::Sequence { children } => children.iter().all(matrix_dimensions_valid),
        MathNode::Superscript { base, exponent } => {
            matrix_dimensions_valid(base) && matrix_dimensions_valid(exponent)
        }
        MathNode::Subscript { base, subscript } => {
            matrix_dimensions_valid(base) && matrix_dimensions_valid(subscript)
        }
        MathNode::SubSuperscript {
            base,
            subscript,
            exponent,
        } => {
            matrix_dimensions_valid(base)
                && matrix_dimensions_valid(subscript)
                && matrix_dimensions_valid(exponent)
        }
        MathNode::Fraction {
            numerator,
            denominator,
        } => matrix_dimensions_valid(numerator) && matrix_dimensions_valid(denominator),
        MathNode::Root { radicand } => matrix_dimensions_valid(radicand),
        MathNode::Equation { left, right } => {
            matrix_dimensions_valid(left) && matrix_dimensions_valid(right)
        }
        MathNode::EquationArray { equations } => equations.iter().all(matrix_dimensions_valid),
        MathNode::Group { child } => matrix_dimensions_valid(child),
        _ => true,
    }
}

fn parse_math(plain: &str, latex: &str) -> MathNode {
    if let Some(matrix) = parse_matrix(plain) {
        return matrix;
    }
    let lines = plain
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() > 1 && lines.iter().all(|line| line.contains('=')) {
        return MathNode::EquationArray {
            equations: lines.iter().map(|line| parse_expression(line)).collect(),
        };
    }
    parse_expression(if latex.trim().is_empty() {
        plain
    } else {
        latex
    })
}

fn parse_matrix(value: &str) -> Option<MathNode> {
    let cleaned = value
        .replace(['', '', '', '', '', ''], "")
        .replace(['[', ']', '(', ')'], "");
    let rows = cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.split_whitespace()
                .filter(|cell| is_matrix_cell(cell))
                .map(parse_atom)
                .collect::<Vec<_>>()
        })
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>();
    if rows.len() < 2 {
        return None;
    }
    let columns = rows[0].len();
    if columns == 0 || rows.iter().any(|row| row.len() != columns) {
        return None;
    }
    Some(MathNode::Matrix {
        rows,
        delimiter: "bmatrix".into(),
    })
}

fn is_matrix_cell(value: &str) -> bool {
    value.chars().count() <= 20
        && value.chars().any(|character| character.is_alphanumeric())
        && !value.contains('=')
}

fn parse_expression(value: &str) -> MathNode {
    if let Some((left, right)) = split_top_level(value, '=') {
        return MathNode::Equation {
            left: Box::new(parse_sequence(left)),
            right: Box::new(parse_latex_fraction(right).unwrap_or_else(|| parse_sequence(right))),
        };
    }
    if let Some(fraction) = parse_latex_fraction(value) {
        return fraction;
    }
    parse_sequence(value)
}

fn parse_latex_fraction(value: &str) -> Option<MathNode> {
    let value = value.trim();
    let rest = value.strip_prefix("\\frac{")?;
    let numerator_end = rest.find('}')?;
    let numerator = &rest[..numerator_end];
    let denominator = rest[numerator_end + 1..].strip_prefix('{')?;
    let denominator_end = denominator.find('}')?;
    if !denominator[denominator_end + 1..].trim().is_empty() {
        return None;
    }
    Some(MathNode::Fraction {
        numerator: Box::new(parse_sequence(numerator)),
        denominator: Box::new(parse_sequence(&denominator[..denominator_end])),
    })
}

fn parse_sequence(value: &str) -> MathNode {
    let mut nodes = Vec::new();
    let chars = value.trim().chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character.is_whitespace() {
            index += 1;
            continue;
        }
        if character.is_ascii_alphanumeric() || character.is_alphabetic() {
            let mut base = parse_atom(&character.to_string());
            index += 1;
            let mut subscript = None;
            let mut exponent = None;
            while index < chars.len() && matches!(chars[index], '^' | '_') {
                let marker = chars[index];
                let (argument, next) = parse_script_argument(&chars, index + 1);
                if marker == '^' {
                    exponent = Some(Box::new(parse_sequence(&argument)));
                } else {
                    subscript = Some(Box::new(parse_sequence(&argument)));
                }
                index = next;
            }
            base = match (subscript, exponent) {
                (Some(subscript), Some(exponent)) => MathNode::SubSuperscript {
                    base: Box::new(base),
                    subscript,
                    exponent,
                },
                (Some(subscript), None) => MathNode::Subscript {
                    base: Box::new(base),
                    subscript,
                },
                (None, Some(exponent)) => MathNode::Superscript {
                    base: Box::new(base),
                    exponent,
                },
                (None, None) => base,
            };
            nodes.push(base);
            continue;
        }
        nodes.push(MathNode::Operator {
            value: character.to_string(),
        });
        index += 1;
    }
    if nodes.len() == 1 {
        nodes.pop().unwrap_or(MathNode::Sequence {
            children: Vec::new(),
        })
    } else {
        MathNode::Sequence { children: nodes }
    }
}

fn parse_atom(value: &str) -> MathNode {
    if value.chars().all(|character| character.is_ascii_digit()) {
        MathNode::Number {
            value: value.into(),
        }
    } else if value.chars().all(|character| character.is_alphabetic()) {
        MathNode::Identifier {
            value: value.into(),
        }
    } else {
        MathNode::Symbol {
            value: value.into(),
        }
    }
}

fn parse_script_argument(chars: &[char], start: usize) -> (String, usize) {
    if chars.get(start) != Some(&'{') {
        return chars
            .get(start)
            .map(|character| (character.to_string(), start + 1))
            // A truncated PDF token such as `A^` must still advance to EOF.
            // Returning the tuple default resets the caller to index zero and
            // causes an unbounded AST allocation loop.
            .unwrap_or_else(|| (String::new(), chars.len()));
    }
    let mut depth = 1usize;
    let mut index = start + 1;
    let mut value = String::new();
    while index < chars.len() {
        match chars[index] {
            '{' => {
                depth += 1;
                value.push('{');
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (value, index + 1);
                }
                value.push('}');
            }
            character => value.push(character),
        }
        index += 1;
    }
    (value, index)
}

fn split_top_level(value: &str, separator: char) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            _ if character == separator && depth == 0 => {
                return Some((&value[..index], &value[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

pub fn to_latex(node: &MathNode) -> String {
    match node {
        MathNode::Symbol { value }
        | MathNode::Identifier { value }
        | MathNode::Number { value }
        | MathNode::Operator { value } => value.clone(),
        MathNode::Sequence { children } => children.iter().map(to_latex).collect(),
        MathNode::Superscript { base, exponent } => {
            format!("{}^{{{}}}", to_latex(base), to_latex(exponent))
        }
        MathNode::Subscript { base, subscript } => {
            format!("{}_{{{}}}", to_latex(base), to_latex(subscript))
        }
        MathNode::SubSuperscript {
            base,
            subscript,
            exponent,
        } => format!(
            "{}_{{{}}}^{{{}}}",
            to_latex(base),
            to_latex(subscript),
            to_latex(exponent)
        ),
        MathNode::Fraction {
            numerator,
            denominator,
        } => {
            format!(
                "\\frac{{{}}}{{{}}}",
                to_latex(numerator),
                to_latex(denominator)
            )
        }
        MathNode::Root { radicand } => format!("\\sqrt{{{}}}", to_latex(radicand)),
        MathNode::Matrix { rows, delimiter } => format!(
            "\\begin{{{delimiter}}}{}\\end{{{delimiter}}}",
            rows.iter()
                .map(|row| row.iter().map(to_latex).collect::<Vec<_>>().join(" & "))
                .collect::<Vec<_>>()
                .join(" \\\\ ")
        ),
        MathNode::Equation { left, right } => format!("{}={}", to_latex(left), to_latex(right)),
        MathNode::EquationArray { equations } => format!(
            "\\begin{{aligned}}{}\\end{{aligned}}",
            equations
                .iter()
                .map(to_latex)
                .collect::<Vec<_>>()
                .join(" \\\\ ")
        ),
        MathNode::Group { child } => format!("({})", to_latex(child)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconstructs_scripts_and_equations() {
        let ast = parse_math("A^T A x = A^T b", "A^{T} A x = A^{T} b");
        assert_eq!(to_latex(&ast), "A^{T}Ax=A^{T}b");
    }

    #[test]
    fn malformed_trailing_script_is_bounded() {
        let ast = parse_math("A^", "A^");
        assert!(to_latex(&ast).len() < 16);
    }

    #[test]
    fn reconstructs_superscript_from_preserved_token_geometry() {
        let block = json!({
            "originalTokens": [
                {"originalText":"A","bbox":[10,20,20,32]},
                {"originalText":"T","bbox":[21,14,27,22]},
                {"originalText":"=","bbox":[30,20,38,32]},
                {"originalText":"I","bbox":[42,20,50,32]}
            ]
        });
        assert_eq!(positioned_token_latex(&block).as_deref(), Some("A^{T}=I"));
    }

    #[test]
    fn groups_multi_glyph_and_negative_superscripts() {
        let exponent = json!({
            "originalTokens": [
                {"originalText":"e","bbox":[10,20,20,32]},
                {"originalText":"A","bbox":[21,14,27,22]},
                {"originalText":"t","bbox":[28,14,34,22]}
            ]
        });
        assert_eq!(positioned_token_latex(&exponent).as_deref(), Some("e^{At}"));
        let inverse = json!({
            "originalTokens": [
                {"originalText":"A","bbox":[10,20,20,32]},
                {"originalText":"−","bbox":[21,14,27,22]},
                {"originalText":"1","bbox":[28,14,34,22]}
            ]
        });
        assert_eq!(positioned_token_latex(&inverse).as_deref(), Some("A^{−1}"));
    }

    #[test]
    fn ordinary_minor_font_variation_is_not_a_superscript() {
        let block = json!({
            "originalTokens": [
                {"originalText":"2","bbox":[10,20,20,32]},
                {"originalText":"c","bbox":[22,19.5,30,30.5]},
                {"originalText":"+","bbox":[32,20,40,32]},
                {"originalText":"3","bbox":[42,20,50,32]}
            ]
        });
        assert_eq!(positioned_token_latex(&block), None);
    }

    #[test]
    fn reconstructs_stacked_native_glyphs_as_a_fraction() {
        let mut block = json!({
            "type":"formula",
            "content":"x=21",
            "formulaConfidence":0.82,
            "originalTokens": [
                {"originalText":"x","bbox":[139.1,483.5,145.2,477.7],"baseline":477.8,"fontSize":12.95,"coordinateSystem":"pdf"},
                {"originalText":"=","bbox":[148.8,482.5,157.4,479.5],"baseline":477.8,"fontSize":12.95,"coordinateSystem":"pdf"},
                {"originalText":"2","bbox":[162.6,479.8,166.9,473.4],"baseline":473.4,"fontSize":9.58,"coordinateSystem":"pdf"},
                {"originalText":"1","bbox":[163.3,489.3,166.0,482.9],"baseline":482.9,"fontSize":9.58,"coordinateSystem":"pdf"}
            ]
        });
        enrich_block(&mut block);
        assert_eq!(block["normalizedLatex"], "x=\\frac{1}{2}");
        assert_eq!(block["math"]["ast"]["right"]["kind"], "fraction");
    }

    #[test]
    fn reconstructs_rectangular_matrix() {
        let ast = parse_math("[1 1\n2 3\n3 4]", "");
        assert_eq!(
            to_latex(&ast),
            "\\begin{bmatrix}1 & 1 \\\\ 2 & 3 \\\\ 3 & 4\\end{bmatrix}"
        );
    }

    #[test]
    fn plain_text_matrix_is_compact_and_preserves_repeated_cells() {
        let ast = parse_math("[1 1\n2 3]", "");
        assert_eq!(to_plain_text(&ast), "[[1, 1], [2, 3]]");
    }

    #[test]
    fn formula_quality_rejects_private_use_glyphs_and_bad_braces() {
        let private = MathNode::Symbol {
            value: "\u{f8ee}".into(),
        };
        assert_eq!(formula_quality(&private, 1.0)["level"], "bad");
        assert!(!validate_latex("x_{1"));
        assert!(validate_latex("x_{1}^{2}"));
    }

    #[test]
    fn matrix_quality_rejects_truncated_cells() {
        let ast = parse_math("[1 1\nJ3 1+]", "");
        assert_eq!(formula_quality(&ast, 1.0)["level"], "bad");
        assert_eq!(formula_quality(&ast, 1.0)["matrixDimensionsValid"], false);
    }

    #[test]
    fn reconstructs_equation_array_atomically() {
        let ast = parse_math("c+d=2\n2c+3d=5\n3c+4d=7", "");
        assert!(matches!(ast, MathNode::EquationArray { .. }));
        assert!(to_latex(&ast).contains("\\begin{aligned}"));
    }
}
