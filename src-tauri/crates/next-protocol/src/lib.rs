pub mod gateway;
mod generated;
pub mod runtime;
pub mod transport;
pub use generated::{
    Call, Manifest, Request, Response, BACKEND_QUEUE, BACKEND_TIMEOUT_MS, CONTRACT_SHA256,
    HANDSHAKE_TIMEOUT_MS, LEASE_MS, MAX_BACKEND_WORKERS, MAX_FRAME_BYTES, MAX_INFLIGHT,
    MAX_LEASE_MS, MAX_STORAGE_CHUNK_BYTES, MAX_STORAGE_TRANSACTION_BYTES,
    MAX_STORAGE_TRANSACTION_OPS, MAX_STORAGE_VALUE_BYTES, MIN_LEASE_MS, RPC_TIMEOUT_MS,
};
use serde_json::Value;
// Generated immutable schema is parsed once per process, never once per frame.
// Payload validation and per-session admission still run for every request.
fn contract() -> &'static Value {
    static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(generated::CONTRACT_JSON).expect("generated Next contract")
    })
}

fn canonical_numeric(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|c| c.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
        && value
            .parse::<u64>()
            .is_ok_and(|n| n <= 9_007_199_254_740_991)
}
fn semver(value: &str) -> bool {
    let (core, pre) = value
        .split_once('-')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    let core: Vec<_> = core.split('.').collect();
    core.len() == 3
        && core.iter().all(|part| canonical_numeric(part))
        && pre.is_none_or(|pre| {
            pre.split('.').all(|id| {
                !id.is_empty()
                    && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    && (!id.bytes().all(|c| c.is_ascii_digit()) || canonical_numeric(id))
            })
        })
}
fn shape(schema: &Value, value: &Value) -> bool {
    if schema["oneOf"]
        .as_array()
        .is_some_and(|choices| choices.iter().filter(|s| shape(s, value)).count() != 1)
    {
        return false;
    }
    if schema.get("const").is_some_and(|expected| {
        expected != value
            && !(expected.is_number() && value.is_number() && expected.as_f64() == value.as_f64())
    }) {
        return false;
    }
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.contains(value))
    {
        return false;
    }
    match schema["type"].as_str() {
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        Some("integer") | Some("number") => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && (schema["type"] != "integer" || n.fract() == 0.0)
                && n >= schema["minimum"].as_f64().unwrap_or(f64::NEG_INFINITY)
                && n <= schema["maximum"].as_f64().unwrap_or(f64::INFINITY)
        }),
        Some("object") => {
            let Some(object) = value.as_object() else {
                return false;
            };
            if schema["required"].as_array().is_some_and(|keys| {
                keys.iter()
                    .any(|key| !object.contains_key(key.as_str().unwrap()))
            }) {
                return false;
            }
            let properties = schema["properties"].as_object();
            if schema["additionalProperties"] == false
                && object
                    .keys()
                    .any(|key| properties.is_none_or(|p| !p.contains_key(key)))
            {
                return false;
            }
            properties.is_none_or(|p| {
                p.iter()
                    .all(|(key, child)| object.get(key).is_none_or(|v| shape(child, v)))
            })
        }
        Some("array") => {
            let Some(items) = value.as_array() else {
                return false;
            };
            items.len() as u64 >= schema["minItems"].as_u64().unwrap_or(0)
                && items.len() as u64 <= schema["maxItems"].as_u64().unwrap_or(u64::MAX)
                && (schema["uniqueItems"] != true
                    || items
                        .iter()
                        .enumerate()
                        .all(|(i, x)| !items[..i].contains(x)))
                && items.iter().all(|item| shape(&schema["items"], item))
        }
        Some("string") => {
            let Some(text) = value.as_str() else {
                return false;
            };
            let length = text.chars().count() as u64;
            if length < schema["minLength"].as_u64().unwrap_or(0)
                || length > schema["maxLength"].as_u64().unwrap_or(u64::MAX)
            {
                return false;
            }
            if schema.get("format").is_some()
                && (schema["format"] != "semver-without-build" || !semver(text))
            {
                return false;
            }
            match schema["pattern"].as_str() {
                Some("^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$") => {
                    if let Some(hex) = text.strip_prefix('#') {
                        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|c| c.is_ascii_hexdigit())
                    } else {
                        text.strip_prefix("rgb(").or_else(|| text.strip_prefix("rgba("))
                            .and_then(|body| body.strip_suffix(')'))
                            .is_some_and(|body| !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',' || c.is_whitespace()))
                    }
                }
                Some("^[^;<>\\\\]+$") => !text.chars().any(|c| matches!(c, ';' | '<' | '>' | '\\')),

                Some("^[A-Za-z0-9_.:-]+$") | Some("^[A-Za-z0-9_.:-]*$") => text
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c)),
                Some("^[a-z][a-z0-9-]{1,63}$") => {
                    text.as_bytes()
                        .first()
                        .is_some_and(|c| c.is_ascii_lowercase())
                        && text
                            .bytes()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                }
                Some("^[A-Za-z][A-Za-z0-9_]{0,63}$") => {
                    text.as_bytes()
                        .first()
                        .is_some_and(|c| c.is_ascii_alphabetic())
                        && text.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                }
                Some("^[A-Za-z0-9]+$") => text.bytes().all(|c| c.is_ascii_alphanumeric()),
                Some("^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$") => {
                    let bytes = text.as_bytes();
                    if bytes.is_empty() || bytes.len() % 4 != 0 {
                        false
                    } else {
                        let padding = if bytes.ends_with(b"==") {
                            2
                        } else if bytes.ends_with(b"=") {
                            1
                        } else {
                            0
                        };
                        let body = &bytes[..bytes.len() - padding];
                        body.iter()
                            .all(|c| c.is_ascii_alphanumeric() || *c == b'+' || *c == b'/')
                            && (padding == 0
                                || (padding == 1 && body.len() % 4 == 3)
                                || (padding == 2 && body.len() % 4 == 2))
                    }
                }
                None => true,
                _ => false,
            }
        }
        None => true,
        _ => false,
    }
}
fn visit(value: &Value, depth: u64, nodes: &mut u64, budget: &Value) -> Result<(), &'static str> {
    *nodes += 1;
    if value.is_number()
        && value
            .as_f64()
            .is_none_or(|n| !n.is_finite() || n.abs() > 9007199254740991.0)
    {
        return Err("INVALID_REQUEST");
    }
    if depth > budget["depth"].as_u64().unwrap() || *nodes > budget["nodes"].as_u64().unwrap() {
        return Err("BUDGET_EXCEEDED");
    }
    match value {
        Value::Array(items) => {
            for item in items {
                visit(item, depth + 1, nodes, budget)?;
            }
        }
        Value::Object(items) => {
            for item in items.values() {
                visit(item, depth + 1, nodes, budget)?;
            }
        }
        _ => (),
    }
    Ok(())
}
/// Decode JSON with shared node/depth/number limits and a caller-selected UTF-8 byte cap.
pub fn validate_json_value(raw: &str, max_bytes: usize) -> Result<Value, &'static str> {
    if raw.len() > max_bytes {
        return Err("BUDGET_EXCEEDED");
    }
    let value: Value = serde_json::from_str(raw).map_err(|_| "INVALID_REQUEST")?;
    let contract = contract();
    visit(&value, 1, &mut 0, &contract["budget"])?;
    Ok(value)
}
/// Validate a stored JSON value, independent of the small RPC envelope budget.
pub fn validate_storage_value(raw: &str) -> Result<Value, &'static str> {
    validate_json_value(raw, generated::MAX_STORAGE_VALUE_BYTES)
}
/// Decode a protocol envelope with the frame byte cap and shared data limits.
pub fn validate_payload(raw: &str) -> Result<Value, &'static str> {
    validate_json_value(raw, generated::MAX_FRAME_BYTES)
}
pub fn validate_request(raw: &str) -> Result<Value, &'static str> {
    let request = validate_payload(raw)?;
    let contract = contract();
    if !shape(&contract["envelope"], &request) {
        return Err("INVALID_REQUEST");
    }
    let method = &contract["methods"][request["method"].as_str().unwrap()];
    if method.is_null() || !shape(&method["params"], &request["params"]) {
        return Err("INVALID_REQUEST");
    }
    Ok(request)
}
pub fn validate_manifest(raw: &str) -> Result<Manifest, &'static str> {
    let mut manifest = validate_payload(raw).map_err(|_| "INVALID_MANIFEST")?;
    let contract = contract();
    if !shape(&contract["manifest"], &manifest) {
        return Err("INVALID_MANIFEST");
    }
    for (key, rule) in contract["manifest"]["properties"].as_object().unwrap() {
        if rule["const"].is_number() {
            manifest[key] = rule["const"].clone();
        }
    }
    serde_json::from_value(manifest).map_err(|_| "INVALID_MANIFEST")
}
pub fn validate_response(raw: &str, request_id: &str) -> Result<Response, &'static str> {
    let mut response = validate_payload(raw).map_err(|_| "INVALID_RESPONSE")?;
    let contract = contract();
    if response["requestId"].as_str() != Some(request_id)
        || !(shape(&contract["responses"]["success"], &response)
            || shape(&contract["responses"]["failure"], &response))
    {
        return Err("INVALID_RESPONSE");
    }
    response["wireVersion"] = contract["versions"]["wireVersion"].clone();
    serde_json::from_value(response).map_err(|_| "INVALID_RESPONSE")
}
pub fn validate_method_result(method: &str, result: &Value) -> bool {
    let contract = contract();
    contract["methods"].get(method).is_some_and(|entry| {
        entry
            .get("result")
            .is_none_or(|schema| shape(schema, result))
    })
}

pub fn required_capability(method: &str) -> Option<Option<String>> {
    let contract = contract();
    let entry = contract["methods"].get(method)?;
    Some(entry["capability"].as_str().map(str::to_owned))
}
pub fn validate_typed_request(raw: &str) -> Result<Request, &'static str> {
    let mut request = validate_request(raw)?;
    request["wireVersion"] = Value::from(3);
    serde_json::from_value(request).map_err(|_| "INVALID_REQUEST")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_fixtures() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../../../../contracts/next/fixtures.json")).unwrap();
        for case in fixtures.as_array().unwrap() {
            let raw = case["raw"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| case["request"].to_string());
            assert_eq!(
                validate_typed_request(&raw).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn shared_response_fixtures() {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../../../contracts/next/response-fixtures.json"
        ))
        .unwrap();
        for case in fixtures.as_array().unwrap() {
            let raw = case["raw"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| case["response"].to_string());
            assert_eq!(
                validate_response(&raw, case["requestId"].as_str().unwrap()).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn shared_manifest_fixtures() {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../../../contracts/next/manifest-fixtures.json"
        ))
        .unwrap();
        for case in fixtures.as_array().unwrap() {
            assert_eq!(
                validate_manifest(&case["manifest"].to_string()).is_ok(),
                case["valid"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn unknown_methods_have_no_capability_entry() {
        assert_eq!(required_capability("db.query"), None);
        assert_eq!(required_capability("runtime.ping"), Some(None));
        assert_eq!(
            required_capability("storage.set"),
            Some(Some("storage:write".into()))
        );
    }
}

pub fn validate_appearance(raw: &str) -> Result<Value, &'static str> {
    let value = validate_payload(raw)?;
    let contract = contract();
    if !shape(&contract["rendererAppearance"], &value) {
        return Err("INVALID_APPEARANCE");
    }
    Ok(value)
}
#[cfg(test)]
#[test]
fn shared_appearance_fixtures() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../contracts/next/appearance-fixtures.json"
    ))
    .unwrap();
    for case in fixtures.as_array().unwrap() {
        assert_eq!(
            validate_appearance(&case["appearance"].to_string()).is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}
