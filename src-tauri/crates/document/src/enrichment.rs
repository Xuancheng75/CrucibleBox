use serde_json::{json, Value};
pub fn enrich_formula_blocks(document: &mut Value) {
    if let Some(pages) = document.get_mut("pages").and_then(Value::as_array_mut) {
        for page in pages {
            let page_number = page["number"].as_u64().unwrap_or(1);
            if let Some(blocks) = page.get_mut("blocks").and_then(Value::as_array_mut) {
                for block in blocks {
                    if block["type"] != "formula" {
                        continue;
                    }
                    if block["source"] == "ocr/formula-model" {
                        block["page"] = json!(page_number);
                        block["atomicBlock"] = json!(true);
                        block["math"] = json!({
                            "rawLatex": block["rawLatex"],
                            "normalizedLatex": block["normalizedLatex"],
                            "display": "display",
                            "quality": "needs-review",
                        });
                        continue;
                    }
                    let plain_text = block["plainText"]
                        .as_str()
                        .or_else(|| block["rawText"].as_str())
                        .or_else(|| block["content"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    let result = crate::formula_ocr::recognize_text(&plain_text);
                    block["plainText"] = json!(plain_text);
                    block["content"] = json!(result.latex.clone());
                    block["latex"] = json!(result.latex);
                    block["rawLatex"] = json!(result.raw_latex);
                    block["normalizedLatex"] = json!(result.normalized_latex);
                    block["formulaEngine"] = json!(result.engine);
                    block["formulaModelVersion"] = json!(result.model_version);
                    block["formulaConfidence"] = json!(result.confidence);
                    block["displayOrInline"] = block["displayOrInline"]
                        .as_str()
                        .map_or_else(|| json!(result.display_or_inline), |mode| json!(mode));
                    block["region"] = json!("formula");
                    block["source"] = block["source"]
                        .as_str()
                        .map_or_else(|| json!("native/pdf/formula_ocr"), |source| json!(source));
                    block["page"] = json!(page_number);
                    crate::document_math::enrich_block(block);
                }
            }
        }
    }
}
