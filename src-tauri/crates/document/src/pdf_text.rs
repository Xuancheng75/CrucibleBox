use serde_json::{json, Value};
pub fn coalesce_native_text_fragments(blocks: &mut Vec<Value>) {
    let mut index = 0usize;
    while index + 1 < blocks.len() {
        if !matches!(blocks[index]["type"].as_str(), Some("text" | "paragraph"))
            || !matches!(
                blocks[index + 1]["type"].as_str(),
                Some("text" | "paragraph")
            )
        {
            index += 1;
            continue;
        }
        let (Some(left_bbox), Some(right_bbox)) =
            (block_bbox(&blocks[index]), block_bbox(&blocks[index + 1]))
        else {
            index += 1;
            continue;
        };
        if !same_text_line(&blocks[index], &blocks[index + 1]) {
            index += 1;
            continue;
        }
        let left_len = blocks[index]["content"]
            .as_str()
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or(0);
        let right_len = blocks[index + 1]["content"]
            .as_str()
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or(0);
        // The high-value failure mode is a short font/math run splitting a
        // logical line. Joining two already substantial runs here changes TOC
        // and heading evidence before structure recovery, so leave those for
        // the paragraph/reading-order stage.
        let continues_short_run = blocks[index]["containsMergedShortRun"] == true;
        if left_len > 8 && right_len > 8 && !continues_short_run {
            index += 1;
            continue;
        }
        let height = (left_bbox[3] - left_bbox[1])
            .abs()
            .max((right_bbox[3] - right_bbox[1]).abs())
            .max(1.0);
        let gap = right_bbox[0] - left_bbox[2];
        if gap < -height || gap > height * 4.0 {
            index += 1;
            continue;
        }
        let left_text = blocks[index]["content"]
            .as_str()
            .unwrap_or_default()
            .trim_end();
        let right_text = blocks[index + 1]["content"]
            .as_str()
            .unwrap_or_default()
            .trim();
        let duplicate = deduplicated_formula_fragments(&blocks[index..=index + 1]).len() == 1
            || (right_len <= 8 && gap <= height * 0.12 && left_text.ends_with(right_text));
        if !duplicate {
            let separator = if gap <= height * 0.18
                || right_text.starts_with(|character: char| ",.;:!?)]}".contains(character))
                || left_text.ends_with(|character: char| "([{/+-−=".contains(character))
            {
                ""
            } else {
                " "
            };
            blocks[index]["content"] = json!(format!("{left_text}{separator}{right_text}"));
            blocks[index]["rawText"] = blocks[index]["content"].clone();
        }
        blocks[index]["bbox"] = json!([
            left_bbox[0].min(right_bbox[0]),
            left_bbox[1].min(right_bbox[1]),
            left_bbox[2].max(right_bbox[2]),
            left_bbox[3].max(right_bbox[3])
        ]);
        blocks[index]["containsMergedShortRun"] =
            json!(continues_short_run || left_len <= 8 || right_len <= 8);
        blocks.remove(index + 1);
    }
    for block in blocks {
        if let Some(value) = block.as_object_mut() {
            value.remove("containsMergedShortRun");
        }
    }
}

pub fn native_block_type(text: &str) -> &'static str {
    let value = text.trim();
    if value.is_empty()
        || value
            .chars()
            .all(|character| character.is_ascii_digit() || " .-()[]".contains(character))
    {
        return "text";
    }
    if looks_like_formula(value) {
        return "formula";
    }
    let lower = value.to_ascii_lowercase();
    if lower == "contents"
        || lower == "preface"
        || lower.starts_with("chapter ")
        || lower.starts_with("appendix ")
        || value.starts_with('第')
        || (value.split_whitespace().next().is_some_and(|token| {
            token
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.')
        }) && value.chars().any(|character| character.is_alphabetic()))
    {
        "heading"
    } else {
        "text"
    }
}

pub fn looks_like_formula(text: &str) -> bool {
    crate::document_layout::is_strict_formula_candidate(text)
}

/// PDFium may expose one mathematical line as many short text objects. Join
/// only adjacent glyph-like objects on the same visual line, and only commit
/// the merge when the complete line passes the strict formula policy. This
/// avoids treating an isolated `A` or `T` as a formula while recovering the
/// common `A T A x = A T b` shape into one IR block.
pub fn coalesce_native_formula_fragments(blocks: &mut Vec<Value>) {
    let mut index = 0usize;
    while index < blocks.len() {
        if !is_formula_fragment(&blocks[index]) {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < blocks.len()
            && is_formula_fragment(&blocks[end])
            && same_formula_line(&blocks[end - 1], &blocks[end])
        {
            end += 1;
        }
        if end.saturating_sub(index) < 3 {
            index += 1;
            continue;
        }
        let fragments = deduplicated_formula_fragments(&blocks[index..end]);
        let original_tokens = formula_original_tokens(&fragments);
        let candidate = if original_tokens.is_empty() {
            fragments
                .iter()
                .filter_map(|block| block["content"].as_str())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            original_tokens
                .iter()
                .filter_map(|token| token["originalText"].as_str())
                .collect::<String>()
        };
        if !looks_like_formula(&candidate) {
            index += 1;
            continue;
        }
        let mut merged = blocks[index].clone();
        merged["type"] = json!("formula");
        merged["region"] = json!("formula");
        merged["source"] = json!("native/pdf/formula");
        merged["content"] = json!(candidate.clone());
        merged["rawText"] = json!(candidate);
        let inline = (index > 0 && same_text_line(&blocks[index - 1], &blocks[index]))
            || (end < blocks.len() && same_text_line(&blocks[end - 1], &blocks[end]));
        merged["displayOrInline"] = json!(if inline { "inline" } else { "display" });
        merged["originalTokens"] =
            if original_tokens.is_empty() {
                Value::Array(fragments
                .iter()
                .map(|block| {
                    let bbox = block.get("bbox").cloned().unwrap_or(Value::Null);
                    let values = bbox.as_array();
                    let center_x = values.and_then(|items| {
                        Some((items.first()?.as_f64()? + items.get(2)?.as_f64()?) / 2.0)
                    });
                    let center_y = values.and_then(|items| {
                        Some((items.get(1)?.as_f64()? + items.get(3)?.as_f64()?) / 2.0)
                    });
                    let height = values.and_then(|items| {
                        Some((items.get(3)?.as_f64()? - items.get(1)?.as_f64()?).abs())
                    });
                    let baseline = values
                        .and_then(|items| items.get(3))
                        .cloned()
                        .unwrap_or(Value::Null);
                    json!({
                        "blockId": block["id"],
                        "originalText": block["content"],
                        "bbox": bbox,
                        "centerX": center_x,
                        "centerY": center_y,
                        "baseline": baseline,
                        "estimatedFontHeight": height,
                        "confidence": block.get("confidence").cloned().unwrap_or(json!(1.0)),
                    })
                })
                .collect())
            } else {
                Value::Array(original_tokens)
            };
        if let Some(bbox) = merged_bbox_values(&blocks[index..end]) {
            merged["bbox"] = json!(bbox);
        }
        blocks[index] = merged;
        for remove_index in (index + 1..end).rev() {
            blocks.remove(remove_index);
        }
    }
}

pub fn formula_original_tokens(blocks: &[&Value]) -> Vec<Value> {
    let mut tokens = blocks
        .iter()
        .flat_map(|block| {
            block["nativeGlyphs"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned()
        })
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return tokens;
    }
    tokens.sort_by(|left, right| {
        left["pageCharIndex"]
            .as_u64()
            .cmp(&right["pageCharIndex"].as_u64())
    });
    tokens.dedup_by(|left, right| left["pageCharIndex"] == right["pageCharIndex"]);
    tokens.sort_by(|left, right| {
        let left_x = left["bbox"][0].as_f64().unwrap_or(0.0);
        let right_x = right["bbox"][0].as_f64().unwrap_or(0.0);
        left_x.total_cmp(&right_x).then_with(|| {
            left["pageCharIndex"]
                .as_u64()
                .cmp(&right["pageCharIndex"].as_u64())
        })
    });
    tokens
}

pub fn deduplicated_formula_fragments(blocks: &[Value]) -> Vec<&Value> {
    let mut kept: Vec<&Value> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let duplicate_overlay = kept.iter().any(|previous| {
            previous["content"] == block["content"]
                && match (block_bbox(previous), block_bbox(block)) {
                    (Some(left), Some(right)) => {
                        let (left_x0, left_x1) = (left[0].min(left[2]), left[0].max(left[2]));
                        let (left_y0, left_y1) = (left[1].min(left[3]), left[1].max(left[3]));
                        let (right_x0, right_x1) = (right[0].min(right[2]), right[0].max(right[2]));
                        let (right_y0, right_y1) = (right[1].min(right[3]), right[1].max(right[3]));
                        let overlap_x = (left_x1.min(right_x1) - left_x0.max(right_x0)).max(0.0);
                        let overlap_y = (left_y1.min(right_y1) - left_y0.max(right_y0)).max(0.0);
                        let left_area =
                            ((left[2] - left[0]).abs() * (left[3] - left[1]).abs()).max(1.0);
                        let right_area =
                            ((right[2] - right[0]).abs() * (right[3] - right[1]).abs()).max(1.0);
                        let area_overlay =
                            overlap_x * overlap_y >= left_area.min(right_area) * 0.80;
                        // PdfPageTextSegment::text() selects every character
                        // intersecting the segment rectangle. At font changes
                        // (notably a smaller math glyph), adjacent segment
                        // rectangles can therefore expose the same short text
                        // window twice even though their full rectangles do
                        // not overlap by 80%. Suppress that duplicate only
                        // when geometry proves a boundary overlap and one
                        // window is materially smaller. Equal-size repeated
                        // matrix cells remain distinct.
                        let left_height = (left[3] - left[1]).abs().max(1.0);
                        let right_height = (right[3] - right[1]).abs().max(1.0);
                        let height_ratio =
                            left_height.min(right_height) / left_height.max(right_height);
                        let boundary_overlap = right_x0
                            <= left_x1 + left_height.max(right_height) * 0.05
                            && right_x1 > left_x1
                            && overlap_y > 0.0
                            && height_ratio <= 0.75;
                        area_overlay || boundary_overlap
                    }
                    _ => false,
                }
        });
        if !duplicate_overlay {
            kept.push(block);
        }
    }
    kept
}

/// Recover vertically stacked matrix rows and aligned equation systems after
/// horizontal glyph coalescing. This remains deliberately conservative: every
/// row must already be a formula, geometry must overlap, and the group must
/// carry either matrix delimiters or complete relations.
pub fn coalesce_native_multiline_math(blocks: &mut Vec<Value>) {
    let mut index = 0usize;
    while index < blocks.len() {
        if blocks[index]["type"] != "formula" {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < blocks.len()
            && end - index < 12
            && blocks[end]["type"] == "formula"
            && vertically_adjacent_math(&blocks[end - 1], &blocks[end])
        {
            end += 1;
        }
        if end - index < 2 {
            index += 1;
            continue;
        }
        let rows = blocks[index..end]
            .iter()
            .filter_map(|block| block["content"].as_str())
            .collect::<Vec<_>>();
        let has_matrix_delimiter = rows.iter().any(|row| {
            row.chars()
                .any(|character| "[]()".contains(character))
        });
        let equation_system = rows.iter().all(|row| {
            row.find(['=', '＝']).is_some_and(|separator| {
                let separator_len = row[separator..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(1);
                row[..separator].chars().any(char::is_alphanumeric)
                    && row[separator + separator_len..]
                        .chars()
                        .any(char::is_alphanumeric)
            })
        });
        if !has_matrix_delimiter && !equation_system {
            index += 1;
            continue;
        }
        let mut merged = blocks[index].clone();
        let content = rows.join("\n");
        merged["content"] = json!(content.clone());
        merged["rawText"] = json!(content);
        merged["displayOrInline"] = json!("display");
        merged["atomicBlock"] = json!(true);
        merged["originalRows"] = Value::Array(blocks[index..end].to_vec());
        if let Some(bbox) = merged_bbox_values(&blocks[index..end]) {
            merged["bbox"] = json!(bbox);
        }
        blocks[index] = merged;
        blocks.drain(index + 1..end);
        index += 1;
    }
}

pub fn vertically_adjacent_math(upper: &Value, lower: &Value) -> bool {
    let (Some(a), Some(b)) = (block_bbox(upper), block_bbox(lower)) else {
        return false;
    };
    let (a_top, a_bottom) = (a[1].min(a[3]), a[1].max(a[3]));
    let (b_top, b_bottom) = (b[1].min(b[3]), b[1].max(b[3]));
    let height = (a_bottom - a_top).max(b_bottom - b_top).max(1.0);
    let center_delta = (((a_top + a_bottom) - (b_top + b_bottom)) / 2.0).abs();
    let overlap_x =
        (a[2].max(a[0]).min(b[2].max(b[0])) - a[0].min(a[2]).max(b[0].min(b[2]))).max(0.0);
    let min_width = (a[2] - a[0]).abs().min((b[2] - b[0]).abs()).max(1.0);
    let left_aligned = (a[0].min(a[2]) - b[0].min(b[2])).abs() <= height * 1.5;
    center_delta >= height * 0.45
        && center_delta <= height * 3.2
        && (overlap_x / min_width >= 0.35 || left_aligned)
}

pub fn is_formula_fragment(block: &Value) -> bool {
    let text = block["content"].as_str().unwrap_or_default().trim();
    !text.is_empty()
        && text.chars().count() <= 4
        && !matches!(
            text.to_ascii_lowercase().as_str(),
            "and"
                | "or"
                | "if"
                | "the"
                | "for"
                | "with"
                | "from"
                | "then"
                | "but"
                | "at"
                | "to"
                | "by"
                | "as"
                | "in"
                | "on"
        )
        && text.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || "αβγδεζηθλμνξπρστφχω=+-−×÷^_()[]{}".contains(character)
        })
        && matches!(block["type"].as_str(), Some("text" | "formula"))
}

pub fn same_text_line(left: &Value, right: &Value) -> bool {
    let (Some(left), Some(right)) = (block_bbox(left), block_bbox(right)) else {
        return false;
    };
    let left_y0 = left[1].min(left[3]);
    let left_y1 = left[1].max(left[3]);
    let right_y0 = right[1].min(right[3]);
    let right_y1 = right[1].max(right[3]);
    let overlap = (left_y1.min(right_y1) - left_y0.max(right_y0)).max(0.0);
    let min_height = (left_y1 - left_y0).min(right_y1 - right_y0).max(1.0);
    overlap / min_height >= 0.45
}

pub fn block_bbox(block: &Value) -> Option<[f32; 4]> {
    let values = block.get("bbox")?.as_array()?;
    (values.len() == 4).then_some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
        values[3].as_f64()? as f32,
    ])
}

pub fn same_formula_line(left: &Value, right: &Value) -> bool {
    let (Some(left_bbox), Some(right_bbox)) = (block_bbox(left), block_bbox(right)) else {
        return false;
    };
    let left_height = (left_bbox[3] - left_bbox[1]).abs().max(1.0);
    let right_height = (right_bbox[3] - right_bbox[1]).abs().max(1.0);
    let left_center = (left_bbox[1] + left_bbox[3]) / 2.0;
    let right_center = (right_bbox[1] + right_bbox[3]) / 2.0;
    let horizontal_gap = right_bbox[0] - left_bbox[2];
    (left_center - right_center).abs() <= left_height.max(right_height) * 1.8
        && horizontal_gap >= -left_height
        && horizontal_gap <= left_height.max(right_height) * 14.0
}

pub fn merged_bbox_values(blocks: &[Value]) -> Option<[f32; 4]> {
    blocks.iter().filter_map(block_bbox).reduce(|left, right| {
        [
            left[0].min(right[0]),
            left[1].min(right[1]),
            left[2].max(right[2]),
            left[3].max(right[3]),
        ]
    })
}
