//! Deterministic document structure recovery built on top of the shared IR.
//!
//! Text acquisition (native PDF text or OCR) is intentionally irrelevant to
//! this module. It classifies regions from content plus layout metadata and
//! constructs the same section tree for every source.

use std::collections::HashMap;

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadingKind {
    Chapter,
    Section(u8),
    NumberedParagraph,
    Exercise,
    None,
}

impl HeadingKind {
    fn name(self) -> &'static str {
        match self {
            Self::Chapter => "chapter_heading",
            Self::Section(_) => "section_heading",
            Self::NumberedParagraph => "numbered_paragraph",
            Self::Exercise => "exercise_number",
            Self::None => "body",
        }
    }
}

#[derive(Debug, Clone)]
struct OutlineRecord {
    id: String,
    parent_id: Option<String>,
    title: String,
    level: u8,
    page: u64,
}

pub fn rebuild(document: &mut Value) {
    mark_repeated_regions(document);
    let mut reading_order = Vec::new();
    let mut records = Vec::new();
    let mut stack: Vec<(u8, String, String)> = Vec::new();
    let Some(pages) = document.get_mut("pages").and_then(Value::as_array_mut) else {
        return;
    };
    let toc_section_candidates = collect_toc_section_candidates(pages);

    for page in pages {
        let page_number = page["number"].as_u64().unwrap_or(1);
        let page_height = page["height"].as_f64().unwrap_or_default() as f32;
        let Some(blocks) = page.get_mut("blocks").and_then(Value::as_array_mut) else {
            continue;
        };
        merge_chapter_fragments(blocks);
        merge_section_fragments(blocks, page_height);
        let heights = blocks.iter().filter_map(block_height).collect::<Vec<_>>();
        let median_height = median(&heights);
        let toc_page = is_toc_page(blocks);
        let exercise_markers = blocks
            .iter()
            .filter(|block| {
                let lower = block["content"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                matches!(
                    lower.as_str(),
                    "exercises" | "problems" | "review exercises"
                ) || lower.starts_with("exercises ")
            })
            .count();
        let numbered_items = blocks
            .iter()
            .filter(|block| has_single_number_prefix(block["content"].as_str().unwrap_or_default()))
            .count();
        let answer_page = blocks.iter().any(|block| {
            let lower = block["content"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            lower.contains("answers to") || lower.starts_with("answer key")
        });
        let exercise_page = exercise_markers > 0 || numbered_items >= 2 || answer_page;

        for block in blocks {
            let Some(id) = block["id"].as_str().map(ToOwned::to_owned) else {
                continue;
            };
            reading_order.push(id.clone());
            let content = block["content"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string();
            if content.is_empty() {
                continue;
            }
            if matches!(
                block["region"].as_str(),
                Some("header" | "footer" | "page_number" | "production_mark")
            ) {
                block["parentId"] = Value::Null;
                block["sectionId"] = Value::Null;
                block["sectionPath"] = Value::Null;
                continue;
            }
            if toc_page && is_toc_title(&content) {
                block["type"] = json!("toc");
                block["region"] = json!("toc");
                block["semanticType"] = json!("toc_title");
                block["parentId"] = Value::Null;
                block["sectionId"] = Value::Null;
                block["sectionPath"] = Value::Null;
                continue;
            }
            if toc_page && is_toc_entry(&content) {
                block["type"] = json!("toc_entry");
                block["region"] = json!("toc");
                block["semanticType"] = json!("toc_entry");
                block["parentId"] = Value::Null;
                block["sectionId"] = Value::Null;
                block["sectionPath"] = Value::Null;
                continue;
            }
            if content.eq_ignore_ascii_case("index") {
                block["type"] = json!("index");
                block["region"] = json!("index");
                block["semanticType"] = json!("index");
                block["parentId"] = Value::Null;
                block["sectionId"] = Value::Null;
                block["sectionPath"] = Value::Null;
                block["excludedFromRag"] = json!(true);
                continue;
            }

            // Captions are structural regions, not prose to be merged into
            // the surrounding paragraph. Keep them in the IR so exporters
            // can attach the next table/figure without affecting the section
            // stack.
            if is_table_caption(&content) {
                block["type"] = json!("table");
                block["region"] = json!("table");
                block["semanticType"] = json!("table_caption");
                attach_section(block, &stack);
                continue;
            }
            if is_figure_caption(&content) {
                block["type"] = json!("figure");
                block["region"] = json!("figure");
                block["semanticType"] = json!("figure_caption");
                attach_section(block, &stack);
                continue;
            }

            let kind = classify(
                &content,
                block,
                page_height,
                median_height,
                exercise_page,
                &toc_section_candidates,
            );
            let original_heading = block["type"] == "heading";
            let heading_candidate =
                original_heading || matches!(kind, HeadingKind::Chapter | HeadingKind::Section(_));
            block["headingCandidate"] = json!(heading_candidate);
            block["headingConfidence"] = json!(heading_confidence(
                &content,
                block,
                page_height,
                median_height,
                &kind,
            ));
            block["headingSignals"] = json!(heading_signals(
                &content,
                block,
                page_height,
                median_height,
                &kind,
            ));
            block["semanticType"] = json!(kind.name());
            match kind {
                HeadingKind::Chapter | HeadingKind::Section(_) => {
                    let level = match kind {
                        HeadingKind::Chapter => 1,
                        HeadingKind::Section(depth) => depth.saturating_add(1).clamp(2, 6),
                        _ => unreachable!(),
                    };
                    while stack
                        .last()
                        .is_some_and(|(current, _, _)| *current >= level)
                    {
                        stack.pop();
                    }
                    let parent_id = stack.last().map(|(_, _, id)| id.clone());
                    block["type"] = json!("heading");
                    block["region"] = json!("heading");
                    block["level"] = json!(level);
                    block["parentId"] = parent_id.clone().map_or(Value::Null, Value::String);
                    block["sectionId"] = json!(id.clone());
                    stack.push((level, content.clone(), id.clone()));
                    block["sectionPath"] = json!(section_path(&stack));
                    block["sectionPathConfidence"] = json!(0.95);
                    records.push(OutlineRecord {
                        id,
                        parent_id,
                        title: content,
                        level,
                        page: page_number,
                    });
                }
                HeadingKind::NumberedParagraph | HeadingKind::Exercise => {
                    block["type"] = json!("list");
                    block["region"] = json!(if kind == HeadingKind::Exercise {
                        "exercise"
                    } else {
                        "text"
                    });
                    attach_section(block, &stack);
                }
                HeadingKind::None => {
                    if block["type"] != "formula"
                        && block["type"] != "matrix"
                        && block["type"] != "table"
                        && block["type"] != "image"
                        && block["type"] != "caption"
                    {
                        block["type"] = json!("paragraph");
                        block["region"] = json!("text");
                    }
                    attach_section(block, &stack);
                }
            }
        }
    }
    document["structure"]["readingOrder"] = json!(reading_order);
    document["structure"]["outline"] = json!(build_outline(&records, None));
}

fn attach_section(block: &mut Value, stack: &[(u8, String, String)]) {
    let section_id = stack.last().map(|(_, _, id)| id.clone());
    block["parentId"] = section_id.clone().map_or(Value::Null, Value::String);
    block["sectionId"] = section_id.map_or(Value::Null, Value::String);
    block["sectionPath"] = if stack.is_empty() {
        Value::Null
    } else {
        json!(section_path(stack))
    };
    block["sectionPathConfidence"] = json!(if stack.is_empty() { 0.0 } else { 0.95 });
}

/// Identify repeated page furniture before heading classification. A running
/// header/footer must remain visible in JSON for diagnostics, but it must not
/// enter the body section stack or RAG chunks.
fn mark_repeated_regions(document: &mut Value) {
    let Some(pages) = document["pages"].as_array() else {
        return;
    };
    let page_count = pages.len();
    if page_count < 2 {
        return;
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for page in pages {
        let height = page["height"].as_f64().unwrap_or_default() as f32;
        let Some(blocks) = page["blocks"].as_array() else {
            continue;
        };
        let mut seen_page = std::collections::HashSet::new();
        for (index, block) in blocks.iter().enumerate() {
            let positional_edge = index < 3 || index + 3 >= blocks.len();
            let content = block["content"].as_str().unwrap_or_default().trim();
            if content.chars().count() < 4 || content.chars().count() > 120 {
                continue;
            }
            let is_edge = block_bbox(block).map_or(positional_edge, |bbox| {
                height <= 0.0 || bbox[1] < height * 0.12 || bbox[3] > height * 0.88
            });
            if is_edge {
                let key = normalize_title_word(content);
                if seen_page.insert(key.clone()) {
                    *counts.entry(key).or_default() += 1;
                }
            }
        }
    }
    let repeated = counts
        .into_iter()
        .filter(|(_, count)| *count >= 3 || (page_count < 10 && *count >= 2))
        .map(|(content, _)| content)
        .collect::<std::collections::HashSet<_>>();
    if repeated.is_empty() {
        return;
    }
    let Some(pages) = document.get_mut("pages").and_then(Value::as_array_mut) else {
        return;
    };
    for page in pages {
        let height = page["height"].as_f64().unwrap_or_default() as f32;
        if let Some(blocks) = page.get_mut("blocks").and_then(Value::as_array_mut) {
            let block_count = blocks.len();
            for (index, block) in blocks.iter_mut().enumerate() {
                let content = block["content"].as_str().unwrap_or_default().trim();
                let key = normalize_title_word(content);
                let edge = block_bbox(block)
                    .map_or(index < 3 || index + 3 >= block_count, |bbox| {
                        height <= 0.0 || bbox[1] < height * 0.12 || bbox[3] > height * 0.88
                    });
                let positional_page_number = edge && is_standalone_page_number(content);
                if !edge || (!repeated.contains(&key) && !positional_page_number) {
                    continue;
                }
                let region = if positional_page_number {
                    "page_number"
                } else if block_bbox(block).is_none_or(|bbox| bbox[1] < height * 0.5) {
                    "header"
                } else {
                    "footer"
                };
                block["region"] = json!(region);
                block["type"] = json!(region);
                block["semanticType"] = json!(region);
                block["excludedFromRag"] = json!(true);
                block["qualityReason"] = json!("repeated_page_furniture");
            }
        }
    }
}

fn is_standalone_page_number(value: &str) -> bool {
    let compact = value.trim();
    if compact.is_empty() || compact.chars().count() > 8 {
        return false;
    }
    let normalized = compact
        .chars()
        .filter(|character| !character.is_whitespace() && !matches!(character, '-' | '–' | '—'))
        .collect::<String>();
    let trimmed = normalized.as_str();
    trimmed.chars().all(|character| character.is_ascii_digit())
        || (trimmed.chars().count() <= 6
            && trimmed
                .chars()
                .all(|character| "ivxlcdmIVXLCDM".contains(character)))
}

fn section_path(stack: &[(u8, String, String)]) -> String {
    stack
        .iter()
        .map(|(_, title, _)| title.as_str())
        .collect::<Vec<_>>()
        .join(" > ")
}

fn classify(
    content: &str,
    block: &Value,
    page_height: f32,
    median_height: f32,
    exercise_page: bool,
    toc_section_candidates: &HashMap<String, String>,
) -> HeadingKind {
    let lower = content.to_ascii_lowercase();
    if exercise_page && (lower.contains("answers to") || lower.starts_with("answer key")) {
        return HeadingKind::Exercise;
    }
    if lower.starts_with("chapter ")
        && content.split_whitespace().count() == 2
        && content == content.to_ascii_uppercase()
    {
        // PDFium exposes the running page furniture "CHAPTER N" as a text
        // segment. A real chapter heading normally includes its title; the
        // bare uppercase form must not reset the section stack.
        return HeadingKind::None;
    }
    if matches!(lower.as_str(), "preface" | "contents" | "index") {
        return HeadingKind::Chapter;
    }
    if content.starts_with('第') && content.contains('章') && content.chars().count() <= 40 {
        return HeadingKind::Chapter;
    }
    if lower.starts_with("chapter ")
        && title_like_after_prefix(content, 2)
        && content.split_whitespace().count() <= 12
        && !is_discursive_reference(content)
        && clean_number_token(content.split_whitespace().nth(1))
    {
        return HeadingKind::Chapter;
    }
    if lower.starts_with("chapter ")
        && content.split_whitespace().count() == 2
        && content
            .split_whitespace()
            .nth(1)
            .is_some_and(|number| number.chars().all(|character| character.is_ascii_digit()))
    {
        return HeadingKind::Chapter;
    }
    if lower.starts_with("appendix ")
        && title_like_after_prefix(content, 1)
        && content.split_whitespace().count() <= 12
        && clean_appendix_token(content.split_whitespace().nth(1))
    {
        return HeadingKind::Chapter;
    }
    if let Some(depth) = dotted_section_depth(content) {
        if title_like_after_prefix(content, 1) && content.split_whitespace().count() <= 14 {
            if is_exercise_like(content, 1) {
                return if exercise_page {
                    HeadingKind::Exercise
                } else {
                    HeadingKind::NumberedParagraph
                };
            }
            if !toc_section_candidates.is_empty()
                && !toc_supports_section(content, toc_section_candidates)
            {
                return if exercise_page {
                    HeadingKind::Exercise
                } else {
                    HeadingKind::NumberedParagraph
                };
            }
            return HeadingKind::Section(depth);
        }
    }
    if lower.starts_with("section ")
        && !is_discursive_reference(content)
        && (title_like_after_prefix(content, 2)
            || (content.split_whitespace().count() == 2
                && clean_section_number_token(content.split_whitespace().nth(1))))
        && clean_section_number_token(content.split_whitespace().nth(1))
    {
        if let Some(depth) = content
            .split_whitespace()
            .nth(1)
            .and_then(dotted_section_depth_token)
        {
            return HeadingKind::Section(depth);
        }
    }
    if has_single_number_prefix(content) {
        return if exercise_page {
            HeadingKind::Exercise
        } else {
            HeadingKind::NumberedParagraph
        };
    }

    // Preserve explicit non-numbered headings only when layout supports them.
    // This prevents ordinary short sentences from becoming sections merely
    // because an upstream parser labelled them as headings.
    let explicit_level = block["level"].as_u64();
    let height = block_height(block).unwrap_or_default();
    let visually_prominent = median_height > 0.0 && height >= median_height * 1.25;
    let layout_heading = block["region"] == "heading";
    let away_from_footer = block_bbox(block)
        .map(|bbox| page_height <= 0.0 || bbox[1] < page_height * 0.9)
        .unwrap_or(true);
    if layout_heading
        && explicit_level.is_some()
        && visually_prominent
        && away_from_footer
        && content.split_whitespace().count() <= 14
        && !content.ends_with('.')
    {
        return HeadingKind::Section(explicit_level.unwrap_or(2).saturating_sub(1) as u8);
    }
    if explicit_level.is_some()
        && visually_prominent
        && away_from_footer
        && content.split_whitespace().count() <= 14
        && !content.ends_with('.')
    {
        return HeadingKind::Section(explicit_level.unwrap_or(2).saturating_sub(1) as u8);
    }
    HeadingKind::None
}

fn is_discursive_reference(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let prefix = if lower.starts_with("chapter ") {
        "chapter "
    } else if lower.starts_with("section ") {
        "section "
    } else {
        return false;
    };
    let remainder = lower.strip_prefix(prefix).unwrap_or_default();
    let mut words = remainder.split_whitespace();
    let Some(number) = words.next() else {
        return false;
    };
    let rest = words.collect::<Vec<_>>().join(" ");
    let first = rest.split_whitespace().next().unwrap_or_default();
    // A numbered title such as "Chapter 1 Matrices" is accepted, while
    // sentence-like references such as "Chapter 8. Finding ..." and
    // "Section 8.3 we will ..." are rejected.
    let sentence_starter = matches!(
        first.trim_matches(|character: char| !character.is_alphabetic()),
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "can"
            | "does"
            | "finding"
            | "gives"
            | "how"
            | "is"
            | "our"
            | "the"
            | "this"
            | "we"
            | "will"
            | "you"
    );
    let punctuated_number = number.ends_with(['.', ':']) || number.contains('.');
    punctuated_number && sentence_starter
}

fn heading_confidence(
    content: &str,
    block: &Value,
    page_height: f32,
    median_height: f32,
    kind: &HeadingKind,
) -> f32 {
    let mut score: f32 = match kind {
        HeadingKind::Chapter => 0.92,
        HeadingKind::Section(_) => 0.84,
        _ => 0.18,
    };
    let height = block_height(block).unwrap_or_default();
    if median_height > 0.0 && height >= median_height * 1.2 {
        score += 0.06;
    }
    if block_bbox(block).is_some_and(|bbox| page_height <= 0.0 || bbox[1] < page_height * 0.88) {
        score += 0.02;
    }
    if content.len() > 100 || content.ends_with(['.', ';', ':']) {
        score -= 0.18;
    }
    score.clamp(0.0, 1.0)
}

fn heading_signals(
    content: &str,
    block: &Value,
    page_height: f32,
    median_height: f32,
    kind: &HeadingKind,
) -> Vec<&'static str> {
    let mut signals = Vec::new();
    if matches!(kind, HeadingKind::Chapter | HeadingKind::Section(_)) {
        signals.push("numbering_pattern");
    }
    if median_height > 0.0 && block_height(block).unwrap_or_default() >= median_height * 1.2 {
        signals.push("font_or_height");
    }
    if block_bbox(block).is_some_and(|bbox| page_height <= 0.0 || bbox[1] < page_height * 0.88) {
        signals.push("page_position");
    }
    if content.split_whitespace().count() <= 14 && !content.ends_with('.') {
        signals.push("short_title");
    }
    if is_discursive_reference(content) {
        signals.push("sentence_like_reference");
    }
    signals
}

fn is_table_caption(content: &str) -> bool {
    let lower = content.trim().to_ascii_lowercase();
    (lower.starts_with("table ") || lower.starts_with("таблица "))
        && content.split_whitespace().count() <= 12
}

fn is_figure_caption(content: &str) -> bool {
    let lower = content.trim().to_ascii_lowercase();
    (lower.starts_with("figure ") || lower.starts_with("fig. "))
        && content.split_whitespace().count() <= 18
}

fn dotted_section_depth(content: &str) -> Option<u8> {
    let token = content.split_whitespace().next()?;
    dotted_section_depth_token(token)
}

fn dotted_section_depth_token(token: &str) -> Option<u8> {
    if token.ends_with([':', ')', ']']) || token.ends_with('.') || !token.contains('.') {
        return None;
    }
    let components = token.split('.').collect::<Vec<_>>();
    if components.len() < 2
        || components.len() > 5
        || components
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(|value| value.is_ascii_digit()))
    {
        return None;
    }
    Some((components.len() - 1) as u8)
}

fn has_single_number_prefix(content: &str) -> bool {
    let Some(token) = content.split_whitespace().next() else {
        return false;
    };
    let number = token.trim_end_matches(['.', ')', ']']);
    !number.is_empty()
        && number.chars().all(|value| value.is_ascii_digit())
        && number.len() < token.len()
        && content.split_whitespace().count() >= 2
}

/// A numbered heading normally starts with a title-cased word.  This small
/// lexical guard is important for PDF text layers: prose often begins with a
/// chapter/section reference such as "Chapter 2 studies ..." or "3.12 gives
/// ...", while real headings in the fixture use title-case labels.
fn title_like_after_prefix(content: &str, prefix_words: usize) -> bool {
    let Some(word) = content.split_whitespace().nth(prefix_words) else {
        return false;
    };
    let Some(first_letter) = word.chars().find(|character| character.is_alphabetic()) else {
        return false;
    };
    (first_letter.is_uppercase() || !first_letter.is_ascii()) && !content.ends_with(['.', ';', ':'])
}

fn clean_number_token(token: Option<&str>) -> bool {
    token.is_some_and(|value| {
        value
            .trim_end_matches(['.', ':'])
            .chars()
            .all(|character| character.is_ascii_digit())
    })
}

fn clean_section_number_token(token: Option<&str>) -> bool {
    token.is_some_and(|value| {
        let trimmed = value.trim_end_matches(['.', ':']);
        let parts = trimmed.split('.').collect::<Vec<_>>();
        parts.len() >= 2
            && parts.iter().all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
            })
    })
}

fn clean_appendix_token(token: Option<&str>) -> bool {
    token.is_some_and(|value| {
        let trimmed = value.trim_end_matches(['.', ':', ')', ']']);
        trimmed.len() == 1
            && trimmed
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_uppercase())
    })
}

fn is_exercise_like(content: &str, prefix_words: usize) -> bool {
    let dotted_number = content
        .split_whitespace()
        .next()
        .and_then(|token| token.split('.').nth(1))
        .and_then(|value| value.parse::<u32>().ok());
    if dotted_number.is_some_and(|value| value >= 20) {
        return true;
    }
    let Some(first) = content.split_whitespace().nth(prefix_words) else {
        return false;
    };
    matches!(
        first
            .trim_matches(|character: char| !character.is_alphabetic())
            .to_ascii_lowercase()
            .as_str(),
        "calculate"
            | "compute"
            | "construct"
            | "by"
            | "describe"
            | "determine"
            | "do"
            | "does"
            | "explain"
            | "factor"
            | "find"
            | "for"
            | "give"
            | "how"
            | "if"
            | "in"
            | "invent"
            | "let"
            | "project"
            | "prove"
            | "show"
            | "solve"
            | "suppose"
            | "there"
            | "true"
            | "using"
            | "use"
            | "verify"
            | "what"
            | "where"
            | "which"
            | "why"
            | "write"
    )
}

fn collect_toc_section_candidates(pages: &[Value]) -> HashMap<String, String> {
    pages
        .iter()
        .filter_map(|page| page.get("blocks").and_then(Value::as_array))
        .filter(|blocks| is_toc_page(blocks))
        .flat_map(|blocks| blocks.iter())
        .filter_map(|block| {
            let content = block["content"].as_str()?.trim();
            if !is_toc_entry(content) {
                return None;
            }
            let number = content.split_whitespace().next()?;
            dotted_section_depth(content)?;
            let title = content
                .split_whitespace()
                .skip(1)
                .filter(|token| !token.trim_matches('.').chars().all(|c| c.is_ascii_digit()))
                .collect::<Vec<_>>()
                .join(" ");
            (!title.is_empty()).then(|| (number.to_string(), title))
        })
        .collect()
}

fn toc_supports_section(content: &str, candidates: &HashMap<String, String>) -> bool {
    let Some(number) = content.split_whitespace().next() else {
        return false;
    };
    let Some(candidate_title) = candidates.get(number) else {
        return false;
    };
    let Some(body_first) = content.split_whitespace().nth(1) else {
        return false;
    };
    let Some(candidate_first) = candidate_title.split_whitespace().next() else {
        return false;
    };
    normalize_title_word(body_first) == normalize_title_word(candidate_first)
}

fn normalize_title_word(word: &str) -> String {
    word.trim_matches(|character: char| !character.is_alphanumeric())
        .to_ascii_lowercase()
}

fn is_toc_page(blocks: &[Value]) -> bool {
    let has_title = blocks.iter().any(|block| {
        matches!(
            block["content"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "contents" | "table of contents"
        )
    });
    let entries = blocks
        .iter()
        .filter(|block| is_toc_entry(block["content"].as_str().unwrap_or_default()))
        .count();
    let leader_entries = blocks
        .iter()
        .filter(|block| {
            let content = block["content"].as_str().unwrap_or_default().trim();
            let trailing_page = content
                .split_whitespace()
                .last()
                .is_some_and(|token| token.trim_matches('.').chars().all(|c| c.is_ascii_digit()));
            trailing_page && content.matches('.').count() >= 3
        })
        .count();
    has_title || (entries >= 3 && (leader_entries >= 2 || entries >= 5))
}

fn is_toc_title(content: &str) -> bool {
    matches!(
        content.trim().to_ascii_lowercase().as_str(),
        "contents" | "table of contents"
    )
}

fn is_toc_entry(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.eq_ignore_ascii_case("preface") {
        return true;
    }
    let trailing_page = trimmed
        .split_whitespace()
        .last()
        .is_some_and(|token| token.trim_matches('.').chars().all(|c| c.is_ascii_digit()));
    let dotted_title = dotted_section_depth(trimmed).is_some()
        && title_like_after_prefix(trimmed, 1)
        && trimmed.split_whitespace().count() <= 14;
    (trailing_page
        && (trimmed.contains("...")
            || trimmed.matches('.').count() >= 3
            || dotted_section_depth(trimmed).is_some()
            || trimmed.to_ascii_lowercase().starts_with("chapter ")
            || trimmed.to_ascii_lowercase().starts_with("review exercises")))
        || dotted_title
        || (has_single_number_prefix(trimmed)
            && title_like_after_prefix(trimmed, 1)
            && trimmed.split_whitespace().count() <= 12)
}

/// PDFium exposes a chapter cover as several adjacent text fragments (for
/// example `Chapter`, `1`, `Matrices and`, `Gaussian Elimination`).  Merge
/// that visual heading before classifying the section tree so the following
/// `1.1` block is attached to Chapter 1 instead of to a stale preface/TOC
/// heading.  Only the exact chapter marker is merged; ordinary body prose is
/// left untouched.
fn merge_chapter_fragments(blocks: &mut Vec<Value>) {
    let Some(chapter_index) = blocks.iter().position(|block| {
        block["content"]
            .as_str()
            .is_some_and(|content| content.trim().eq_ignore_ascii_case("chapter"))
    }) else {
        return;
    };
    let Some(number_index) = (chapter_index + 1..blocks.len()).find(|index| {
        let content = blocks[*index]["content"]
            .as_str()
            .unwrap_or_default()
            .trim();
        !content.is_empty() && content.chars().all(|character| character.is_ascii_digit())
    }) else {
        return;
    };
    if number_index > chapter_index + 2 {
        return;
    }
    let number = blocks[number_index]["content"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut title_indices = Vec::new();
    for (index, block) in blocks.iter().enumerate().skip(number_index + 1).take(4) {
        let content = block["content"].as_str().unwrap_or_default().trim();
        if content.is_empty() || dotted_section_depth(content).is_some() {
            break;
        }
        if content.chars().any(|character| character.is_alphabetic()) {
            title_indices.push(index);
        } else {
            break;
        }
    }
    if title_indices.is_empty() {
        return;
    }
    let title = join_layout_fragments(title_indices.iter().map(|index| &blocks[*index]));
    if title.is_empty() {
        return;
    }
    let mut merged = blocks[chapter_index].clone();
    merged["type"] = json!("heading");
    merged["content"] = json!(format!("Chapter {number} {title}"));
    if let Some(bbox) = merged_bbox(
        title_indices
            .iter()
            .copied()
            .chain([number_index, chapter_index])
            .map(|index| &blocks[index]),
    ) {
        merged["bbox"] = json!(bbox);
    }
    blocks[chapter_index] = merged;
    let mut remove = title_indices;
    remove.push(number_index);
    remove.sort_unstable_by(|left, right| right.cmp(left));
    for index in remove {
        blocks.remove(index);
    }
}

fn merged_bbox<'a>(blocks: impl Iterator<Item = &'a Value>) -> Option<[f32; 4]> {
    let mut result: Option<[f32; 4]> = None;
    for block in blocks {
        let bbox = block_bbox(block)?;
        result = Some(match result {
            Some(current) => [
                current[0].min(bbox[0]),
                current[1].min(bbox[1]),
                current[2].max(bbox[2]),
                current[3].max(bbox[3]),
            ],
            None => bbox,
        });
    }
    result
}

/// Merge the horizontally adjacent pieces of a numbered section title. PDF
/// text extraction commonly returns `1.5 Tr` and `Triangular Factors ...` as
/// separate blocks. Formula-like short fragments (`A`, `Ax`, `=`) are left
/// separate so they can still be handled as formula/body regions.
fn merge_section_fragments(blocks: &mut Vec<Value>, page_height: f32) {
    let mut index = 0;
    while index < blocks.len() {
        let content = blocks[index]["content"].as_str().unwrap_or_default().trim();
        let is_header_prefix = content.split_whitespace().count() == 1
            && block_bbox(&blocks[index])
                .is_some_and(|bbox| page_height > 0.0 && bbox[1] > page_height * 0.9);
        if dotted_section_depth(content).is_none() || is_header_prefix {
            index += 1;
            continue;
        }
        let mut fragment_indices = Vec::new();
        let mut previous_index = index;
        for next_index in index + 1..blocks.len().min(index + 5) {
            let next_content = blocks[next_index]["content"]
                .as_str()
                .unwrap_or_default()
                .trim();
            if next_content.is_empty()
                || dotted_section_depth(next_content).is_some()
                || !same_layout_line(&blocks[previous_index], &blocks[next_index])
                || !can_merge_title_fragment(
                    next_content,
                    &blocks[previous_index],
                    &blocks[next_index],
                )
            {
                break;
            }
            fragment_indices.push(next_index);
            previous_index = next_index;
        }
        if fragment_indices.is_empty() {
            index += 1;
            continue;
        }
        let mut all_indices = vec![index];
        all_indices.extend(fragment_indices.iter().copied());
        let mut merged = blocks[index].clone();
        merged["content"] = json!(join_layout_fragments(
            all_indices.iter().map(|item| &blocks[*item])
        ));
        if let Some(bbox) = merged_bbox(all_indices.iter().map(|item| &blocks[*item])) {
            merged["bbox"] = json!(bbox);
        }
        blocks[index] = merged;
        for remove_index in fragment_indices.into_iter().rev() {
            blocks.remove(remove_index);
        }
        index += 1;
    }
}

fn join_layout_fragments<'a>(blocks: impl Iterator<Item = &'a Value>) -> String {
    let mut result = String::new();
    let mut previous_bbox: Option<[f32; 4]> = None;
    for block in blocks {
        let text = block["content"].as_str().unwrap_or_default().trim();
        if text.is_empty() {
            continue;
        }
        let bbox = block_bbox(block);
        let gap = previous_bbox
            .zip(bbox)
            .map(|(left, right)| right[0] - left[2]);
        let joins_word = gap.is_some_and(|value| value <= 2.5)
            && result
                .chars()
                .last()
                .is_some_and(|character| character.is_alphabetic())
            && text
                .chars()
                .next()
                .is_some_and(|character| character.is_alphabetic());
        let overlap = if joins_word {
            shared_prefix_suffix(&result, text)
        } else {
            0
        };
        if !result.is_empty() && !joins_word {
            result.push(' ');
        }
        result.extend(text.chars().skip(overlap));
        previous_bbox = bbox;
    }
    result
}

fn shared_prefix_suffix(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    (1..=left.len().min(right.len()))
        .rev()
        .find(|length| left[left.len() - length..] == right[..*length])
        .unwrap_or(0)
}

fn same_layout_line(left: &Value, right: &Value) -> bool {
    let (Some(left_bbox), Some(right_bbox)) = (block_bbox(left), block_bbox(right)) else {
        return false;
    };
    let left_center = (left_bbox[1] + left_bbox[3]) / 2.0;
    let right_center = (right_bbox[1] + right_bbox[3]) / 2.0;
    (left_center - right_center).abs() <= 6.0 && right_bbox[0] >= left_bbox[0] - 2.0
}

fn can_merge_title_fragment(content: &str, left: &Value, right: &Value) -> bool {
    if !content.chars().any(|character| character.is_alphabetic()) {
        return false;
    }
    let Some((left_bbox, right_bbox)) = block_bbox(left).zip(block_bbox(right)) else {
        return false;
    };
    let gap = right_bbox[0] - left_bbox[2];
    let first = content.chars().find(|character| character.is_alphabetic());
    let short_formula_fragment = content.chars().count() <= 3
        && !matches!(content.to_ascii_lowercase().as_str(), "the" | "and" | "for");
    gap <= 2.5
        || (first.is_some_and(|character| character.is_uppercase()) && !short_formula_fragment)
}

fn block_bbox(block: &Value) -> Option<[f32; 4]> {
    let values = block.get("bbox")?.as_array()?;
    (values.len() == 4).then_some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
        values[3].as_f64()? as f32,
    ])
}

fn block_height(block: &Value) -> Option<f32> {
    block_bbox(block).map(|bbox| (bbox[3] - bbox[1]).abs())
}

fn median(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    // Use the lower middle for even-sized pages so a single large title does
    // not become the baseline that hides its own visual prominence.
    sorted[(sorted.len() - 1) / 2]
}

fn build_outline(records: &[OutlineRecord], parent_id: Option<&str>) -> Vec<Value> {
    records
        .iter()
        .filter(|record| record.parent_id.as_deref() == parent_id)
        .map(|record| {
            json!({
                "id": record.id,
                "title": record.title,
                "level": record.level,
                "page": record.page,
                "parentId": record.parent_id,
                "children": build_outline(records, Some(&record.id))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toc_and_numbered_paragraphs_do_not_pollute_sections() {
        let mut document = json!({
            "pages": [
                {"number":1,"width":612,"height":792,"blocks":[
                    {"id":"toc","type":"heading","content":"Contents","bbox":[20,20,200,40]},
                    {"id":"toc1","type":"heading","content":"1.1 Introduction ........ 4","bbox":[20,60,400,72]}
                ]},
                {"number":2,"width":612,"height":792,"blocks":[
                    {"id":"c1","type":"text","content":"Chapter 1 Matrices and Gaussian Elimination","bbox":[20,20,500,44]},
                    {"id":"s1","type":"text","content":"1.1 Introduction","bbox":[20,70,220,90]},
                    {"id":"p1","type":"heading","content":"6. The question is how to use those six numbers to solve the system.","bbox":[20,110,550,122]}
                ]}
            ], "structure": {}
        });
        rebuild(&mut document);
        assert_eq!(document["pages"][0]["blocks"][1]["type"], "toc_entry");
        assert_eq!(
            document["pages"][1]["blocks"][2]["semanticType"],
            "numbered_paragraph"
        );
        assert_eq!(
            document["pages"][1]["blocks"][2]["sectionPath"],
            "Chapter 1 Matrices and Gaussian Elimination > 1.1 Introduction"
        );
        assert_eq!(
            document["structure"]["outline"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            document["structure"]["outline"][0]["children"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn exercises_are_not_headings() {
        let mut document = json!({"pages":[{"number":1,"height":792,"blocks":[
            {"id":"e","content":"Exercises","bbox":[10,10,100,30]},
            {"id":"q","content":"23. Project the vector onto the line.","bbox":[10,50,300,62]}
        ]}],"structure":{}});
        rebuild(&mut document);
        assert_eq!(
            document["pages"][0]["blocks"][1]["semanticType"],
            "exercise_number"
        );
        assert_eq!(document["pages"][0]["blocks"][1]["type"], "list");
    }

    #[test]
    fn dotted_exercises_and_figure_labels_do_not_become_sections() {
        let mut document = json!({"pages":[{"number":1,"height":792,"blocks":[
            {"id":"figure","content":"1.4: The column picture: linear combination of columns","bbox":[10,100,500,112]},
            {"id":"exercise","content":"1.2 By giving a basis, describe a two-dimensional subspace","bbox":[10,140,500,152]},
            {"id":"long-exercise","content":"5.29 If the vectors are linearly independent, show why","bbox":[10,180,500,192]}
        ]}],"structure":{}});
        rebuild(&mut document);
        let blocks = document["pages"][0]["blocks"].as_array().unwrap();
        assert!(blocks.iter().all(|block| block["type"] != "heading"));
    }

    #[test]
    fn exercise_question_words_are_not_section_headings() {
        for content in [
            "1.5 Factor the preceding matrices into",
            "1.16 For which values of",
        ] {
            let mut document = json!({
                "pages": [{"number": 1, "height": 792, "blocks": [
                    {"id": "chapter", "content": "Chapter 1 Matrices", "bbox": [10, 10, 300, 30]},
                    {"id": "section", "content": "1.1 Introduction", "bbox": [10, 40, 300, 55]},
                    {"id": "exercise", "content": content, "bbox": [10, 70, 500, 82]}
                ]}],
                "structure": {}
            });
            rebuild(&mut document);
            assert_ne!(document["pages"][0]["blocks"][2]["type"], "heading");
        }
    }

    #[test]
    fn prose_references_to_chapters_and_sections_do_not_poison_tree() {
        let mut document = json!({
            "pages": [{ "number": 1, "height": 792, "blocks": [
                { "id": "chapter", "content": "Chapter 1 Matrices", "bbox": [10, 10, 300, 30] },
                { "id": "section", "content": "1.1 Introduction", "bbox": [10, 40, 300, 55] },
                { "id": "chapter-ref", "type": "heading", "content": "Chapter 8. Finding an antiderivative is discussed below.", "bbox": [10, 70, 500, 82] },
                { "id": "section-ref", "type": "heading", "content": "Section 8.3 we will see this later.", "bbox": [10, 90, 500, 102] },
                { "id": "section-ref-2", "type": "heading", "content": "Section 3.8 and our knowledge is incomplete.", "bbox": [10, 110, 500, 122] },
                { "id": "body", "content": "As discussed in Chapter 6, this follows from the definition.", "bbox": [10, 130, 500, 142] }
            ] }],
            "structure": {}
        });
        rebuild(&mut document);
        let blocks = document["pages"][0]["blocks"].as_array().unwrap();
        for id in ["chapter-ref", "section-ref", "section-ref-2", "body"] {
            let block = blocks.iter().find(|block| block["id"] == id).unwrap();
            assert_ne!(block["type"], "heading", "{id} was misclassified");
        }
        assert_eq!(
            blocks[1]["sectionPath"],
            "Chapter 1 Matrices > 1.1 Introduction"
        );
    }

    #[test]
    fn visual_ocr_heading_survives_without_numbering() {
        let mut document = json!({
            "pages": [{ "number": 1, "height": 1000, "blocks": [
                { "id": "title", "type": "heading", "region": "heading", "level": 1,
                  "content": "潮汐灯笼花", "bbox": [80, 80, 520, 130] },
                { "id": "body", "type": "text", "region": "text",
                  "content": "LANTERNA TIDALIS grows near the water.", "bbox": [80, 170, 520, 188] }
            ] }],
            "structure": {}
        });
        rebuild(&mut document);
        let blocks = document["pages"][0]["blocks"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "heading");
        assert_eq!(blocks[0]["semanticType"], "section_heading");
        assert_eq!(blocks[1]["parentId"], "title");
        assert_eq!(blocks[1]["sectionPath"], "潮汐灯笼花");
    }

    #[test]
    fn repeated_headers_and_sequential_page_numbers_stay_out_of_body() {
        let pages = (100..=102)
            .map(|page| json!({
                "number": page,
                "height": 1000,
                "blocks": [
                    {"id": format!("h{page}"), "content":"Chapter 3 Orthogonality", "bbox":[20,20,400,38]},
                    {"id": format!("b{page}"), "content":"Body text remains available.", "bbox":[20,100,500,120]},
                    {"id": format!("n{page}"), "content":page.to_string(), "bbox":[290,970,320,985]}
                ]
            }))
            .collect::<Vec<_>>();
        let mut document = json!({"pages":pages,"structure":{}});
        rebuild(&mut document);
        for page in document["pages"].as_array().unwrap() {
            assert_eq!(page["blocks"][0]["type"], "header");
            assert_eq!(page["blocks"][0]["excludedFromRag"], true);
            assert_eq!(page["blocks"][2]["type"], "page_number");
            assert_eq!(page["blocks"][2]["excludedFromRag"], true);
        }
    }

    #[test]
    fn roman_page_numbers_are_supported() {
        assert!(is_standalone_page_number("vii"));
        assert!(is_standalone_page_number("- XII -"));
        assert!(!is_standalone_page_number("MATRIX"));
    }
}
