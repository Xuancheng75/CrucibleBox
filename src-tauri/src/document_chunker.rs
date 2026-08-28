//! Structure/semantic/hybrid chunking for the unified Document model.

use serde_json::{json, Value};

const DEFAULT_TARGET_TOKENS: usize = 512;
const DEFAULT_MAX_TOKENS: usize = 1024;
const DEFAULT_OVERLAP: usize = 50;
const DEFAULT_MIN_CHARS: usize = 100;

#[derive(Debug, Clone, Copy)]
struct ChunkOptions {
    strategy: Strategy,
    target_tokens: usize,
    max_tokens: usize,
    overlap: usize,
    min_chunk_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Structure,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone)]
struct BlockInput {
    id: String,
    content: String,
    block_type: String,
    page: usize,
}

/// Chunk a unified document. Token counts are deterministic character-based
/// estimates; an embedding/tokenizer-specific count can replace this later
/// without changing the output contract.
pub fn chunk_document(document: &Value, raw_options: Option<&Value>) -> Result<Value, String> {
    let options = parse_options(raw_options)?;
    let document_id = document
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("document")
        .to_string();
    let source = document.get("source").cloned().unwrap_or_else(|| json!({}));
    let source_path = source
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let source_file = source_path
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("document")
        .to_string();
    let blocks = flatten_blocks(document);
    let chunks = build_chunks(&blocks, &document_id, &source_file, &source_path, options);
    Ok(json!({
        "documentId": document_id,
        "strategy": options.strategy.as_str(),
        "chunks": chunks,
        "count": chunks.len()
    }))
}

fn parse_options(raw: Option<&Value>) -> Result<ChunkOptions, String> {
    let object = raw.and_then(Value::as_object);
    let strategy = match object
        .and_then(|value| value.get("strategy"))
        .and_then(Value::as_str)
        .unwrap_or("hybrid")
    {
        "structure" => Strategy::Structure,
        "semantic" => Strategy::Semantic,
        "hybrid" => Strategy::Hybrid,
        other => return Err(format!("unsupported chunk strategy: {other}")),
    };
    let number = |name: &str, default: usize| {
        object
            .and_then(|value| value.get(name))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(default)
    };
    let target_tokens = number("targetTokens", DEFAULT_TARGET_TOKENS);
    let max_tokens = number("maxTokens", DEFAULT_MAX_TOKENS);
    let overlap = number("overlap", DEFAULT_OVERLAP);
    let min_chunk_size = number("minChunkSize", DEFAULT_MIN_CHARS);
    if !(1..=32_768).contains(&target_tokens)
        || !(1..=65_536).contains(&max_tokens)
        || max_tokens < target_tokens
        || overlap >= max_tokens
        || min_chunk_size > 1_000_000
    {
        return Err(
            "invalid chunk options: require 1 <= targetTokens <= maxTokens, overlap < maxTokens"
                .into(),
        );
    }
    Ok(ChunkOptions {
        strategy,
        target_tokens,
        max_tokens,
        overlap,
        min_chunk_size,
    })
}

fn flatten_blocks(document: &Value) -> Vec<BlockInput> {
    let mut blocks = Vec::new();
    let Some(pages) = document.get("pages").and_then(Value::as_array) else {
        return blocks;
    };
    for page in pages {
        let page_number = page.get("number").and_then(Value::as_u64).unwrap_or(1) as usize;
        let Some(page_blocks) = page.get("blocks").and_then(Value::as_array) else {
            continue;
        };
        for (index, block) in page_blocks.iter().enumerate() {
            let Some(content) = block.get("content").and_then(Value::as_str) else {
                continue;
            };
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            blocks.push(BlockInput {
                id: block
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("p{page_number}-b{}", index + 1)),
                content: content.to_string(),
                block_type: block
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("paragraph")
                    .to_string(),
                page: page_number,
            });
        }
    }
    blocks
}

fn build_chunks(
    blocks: &[BlockInput],
    document_id: &str,
    source_file: &str,
    source_path: &str,
    options: ChunkOptions,
) -> Vec<Value> {
    let mut chunks = Vec::new();
    let mut current: Vec<&BlockInput> = Vec::new();
    let mut current_tokens = 0usize;
    let mut chunk_index = 0usize;
    for block in blocks {
        let block_tokens = estimate_tokens(&block.content);
        let starts_structure = matches!(options.strategy, Strategy::Structure | Strategy::Hybrid)
            && block.block_type == "heading";
        // A proof is semantically owned by the immediately preceding theorem
        // (and a formula/definition block is likewise structural context).
        // Keep that pair together even when the proof heading would normally
        // start a new structure chunk.  This prevents RAG chunks from losing
        // the theorem statement that gives a proof its meaning.
        let math_continuation =
            is_proof_heading(block) && current.iter().any(|item| is_theorem_heading(item));
        let proof_body_continuation = block.block_type != "heading"
            && current.iter().any(|item| is_proof_heading(item))
            && current.iter().any(|item| is_theorem_heading(item));
        let should_flush = !current.is_empty()
            && !math_continuation
            && !proof_body_continuation
            && (starts_structure || current_tokens + block_tokens > options.target_tokens);
        if should_flush {
            push_chunk(
                &mut chunks,
                &mut chunk_index,
                document_id,
                source_file,
                source_path,
                &current,
                options,
            );
            current.clear();
            current_tokens = 0;
        }
        if block_tokens > options.max_tokens {
            if !current.is_empty() {
                push_chunk(
                    &mut chunks,
                    &mut chunk_index,
                    document_id,
                    source_file,
                    source_path,
                    &current,
                    options,
                );
                current.clear();
                current_tokens = 0;
            }
            for part in split_long_block(block, options.max_tokens, options.overlap) {
                let synthetic = BlockInput {
                    id: block.id.clone(),
                    content: part,
                    block_type: block.block_type.clone(),
                    page: block.page,
                };
                push_chunk(
                    &mut chunks,
                    &mut chunk_index,
                    document_id,
                    source_file,
                    source_path,
                    &[&synthetic],
                    options,
                );
            }
            continue;
        }
        current.push(block);
        current_tokens += block_tokens;
    }
    if !current.is_empty() {
        push_chunk(
            &mut chunks,
            &mut chunk_index,
            document_id,
            source_file,
            source_path,
            &current,
            options,
        );
    }
    chunks
}

fn push_chunk(
    chunks: &mut Vec<Value>,
    chunk_index: &mut usize,
    document_id: &str,
    source_file: &str,
    source_path: &str,
    blocks: &[&BlockInput],
    options: ChunkOptions,
) {
    if blocks.is_empty() {
        return;
    }
    let content = blocks
        .iter()
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let is_small = content.chars().count() < options.min_chunk_size;
    let page_start = blocks.first().map(|block| block.page).unwrap_or(1);
    let page_end = blocks.last().map(|block| block.page).unwrap_or(page_start);
    let block_ids = blocks
        .iter()
        .map(|block| block.id.clone())
        .collect::<Vec<_>>();
    let block_type = blocks
        .iter()
        .find(|block| block.block_type == "heading")
        .map(|_| "text")
        .unwrap_or_else(|| {
            blocks
                .first()
                .map(|block| block.block_type.as_str())
                .unwrap_or("text")
        });
    chunks.push(json!({
        "chunk_id": format!("{document_id}-c{chunk_index}"),
        "document_id": document_id,
        "parent_id": Value::Null,
        "chunk_index": *chunk_index,
        "title": blocks.iter().find(|block| block.block_type == "heading").map(|block| block.content.clone()),
        "content": content,
        "page_start": page_start,
        "page_end": page_end,
        "source_file": source_file,
        "source_path": source_path,
        "block_ids": block_ids,
        "token_count": estimate_tokens(&content),
        "character_count": content.chars().count(),
        "type": block_type,
        "isSmall": is_small,
    }));
    *chunk_index += 1;
}

fn split_long_block(block: &BlockInput, max_tokens: usize, overlap: usize) -> Vec<String> {
    let max_chars = max_tokens.saturating_mul(4).max(1);
    let overlap_chars = overlap.saturating_mul(4).min(max_chars.saturating_sub(1));
    let chars = block.content.chars().collect::<Vec<_>>();
    let mut result = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + max_chars).min(chars.len());
        result.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(overlap_chars);
    }
    result
}

fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

fn is_theorem_heading(block: &BlockInput) -> bool {
    block.block_type == "heading"
        && ["定理", "引理", "命题", "lemma", "theorem", "proposition"]
            .iter()
            .any(|keyword| block.content.to_ascii_lowercase().contains(keyword))
}

fn is_proof_heading(block: &BlockInput) -> bool {
    block.block_type == "heading"
        && ["证明", "proof"]
            .iter()
            .any(|keyword| block.content.to_ascii_lowercase().contains(keyword))
}

impl Strategy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Value {
        json!({
            "id": "doc-1",
            "source": { "path": "C:/input.md" },
            "pages": [{ "number": 1, "blocks": [
                { "id": "b1", "type": "heading", "content": "第一章" },
                { "id": "b2", "type": "paragraph", "content": "这是正文内容。" },
                { "id": "b3", "type": "paragraph", "content": "第二段。" }
            ]}]
        })
    }

    #[test]
    fn hybrid_preserves_heading_and_emits_contract_fields() {
        let output = chunk_document(&document(), None).unwrap();
        assert_eq!(output["strategy"], "hybrid");
        assert_eq!(output["count"], 1);
        assert_eq!(output["chunks"][0]["title"], "第一章");
        assert_eq!(output["chunks"][0]["block_ids"][0], "b1");
    }

    #[test]
    fn invalid_options_are_rejected() {
        let error = chunk_document(
            &document(),
            Some(&json!({ "maxTokens": 1, "targetTokens": 2 })),
        )
        .unwrap_err();
        assert!(error.contains("invalid chunk options"));
    }

    #[test]
    fn long_block_is_split_without_invalid_utf8() {
        let mut long = String::new();
        for _ in 0..400 {
            long.push('中');
        }
        let document = json!({
            "id": "doc-long",
            "source": { "path": "a.txt" },
            "pages": [{ "number": 1, "blocks": [{ "id": "b", "type": "paragraph", "content": long }]}]
        });
        let output = chunk_document(
            &document,
            Some(&json!({ "targetTokens": 8, "maxTokens": 16, "overlap": 2 })),
        )
        .unwrap();
        assert!(output["count"].as_u64().unwrap() > 1);
    }

    #[test]
    fn theorem_and_proof_stay_in_one_hybrid_chunk() {
        let document = json!({
            "id": "doc-math",
            "source": { "path": "math.md" },
            "pages": [{ "number": 1, "blocks": [
                { "id": "t", "type": "heading", "content": "定理 1" },
                { "id": "ts", "type": "paragraph", "content": "若 a=b，则..." },
                { "id": "p", "type": "heading", "content": "证明" },
                { "id": "ps", "type": "paragraph", "content": "由等式性质可得。" }
            ]}]
        });
        let output = chunk_document(
            &document,
            Some(
                &json!({ "strategy": "hybrid", "targetTokens": 8, "maxTokens": 32, "overlap": 2 }),
            ),
        )
        .unwrap();
        let chunks = output["chunks"].as_array().unwrap();
        assert_eq!(chunks.len(), 1);
        let content = chunks[0]["content"].as_str().unwrap();
        assert!(content.contains("定理 1"));
        assert!(content.contains("证明"));
    }
}
