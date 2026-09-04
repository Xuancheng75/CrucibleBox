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
            ))
        })
        .collect::<Vec<_>>();
    if positioned.len() < 2 {
        return None;
    }
    positioned.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut centers = positioned.iter().map(|token| token.1).collect::<Vec<_>>();
    centers.sort_by(f64::total_cmp);
    let baseline_center = centers[centers.len() / 2];
    let mut heights = positioned.iter().map(|token| token.2).collect::<Vec<_>>();
    heights.sort_by(f64::total_cmp);
    let body_height = heights[heights.len() / 2].max(1.0);
    let mut output = String::new();
    let mut spatial_script = false;
    for (_, center_y, height, text) in positioned {
        // Font fallback and hinting can vary ordinary glyph heights by around
        // ten percent. Require a materially smaller glyph and a clear vertical
        // displacement before creating script structure.
        let small = height <= body_height * 0.78;
        if small && center_y < baseline_center - body_height * 0.22 && !output.is_empty() {
            output.push_str("^{");
            output.push_str(&text);
            output.push('}');
            spatial_script = true;
        } else if small && center_y > baseline_center + body_height * 0.22 && !output.is_empty() {
            output.push_str("_{");
            output.push_str(&text);
            output.push('}');
            spatial_script = true;
        } else {
            output.push_str(&text);
        }
    }
    spatial_script.then_some(output)
}

pub fn latex_from_block(block: &Value) -> Option<String> {
    let ast = block.get("math")?.get("ast")?.clone();
    serde_json::from_value::<MathNode>(ast)
        .ok()
        .map(|node| to_latex(&node))
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
            right: Box::new(parse_sequence(right)),
        };
    }
    parse_sequence(value)
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
    fn reconstructs_rectangular_matrix() {
        let ast = parse_math("[1 1\n2 3\n3 4]", "");
        assert_eq!(
            to_latex(&ast),
            "\\begin{bmatrix}1 & 1 \\\\ 2 & 3 \\\\ 3 & 4\\end{bmatrix}"
        );
    }

    #[test]
    fn reconstructs_equation_array_atomically() {
        let ast = parse_math("c+d=2\n2c+3d=5\n3c+4d=7", "");
        assert!(matches!(ast, MathNode::EquationArray { .. }));
        assert!(to_latex(&ast).contains("\\begin{aligned}"));
    }
}
