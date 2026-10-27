use serde_json::Value;
use std::collections::HashSet;

pub fn package_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_./-".contains(&c))
        && path.split('/').all(|part| {
            let name = part.split('.').next().unwrap_or("").to_ascii_lowercase();
            let reserved = matches!(name.as_str(), "con" | "prn" | "aux" | "nul")
                || (name.len() == 4
                    && (name.starts_with("com") || name.starts_with("lpt"))
                    && matches!(name.as_bytes()[3], b'1'..=b'9'));
            !part.is_empty() && part != "." && part != ".." && !part.ends_with('.') && !reserved
        })
}
fn format(name: &str, value: &str) -> bool {
    match name {
        "plugin-id" => {
            value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && value
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        }
        "package-path" => package_path(value),
        "semver-without-build" => semver::Version::parse(value).is_ok_and(|v| v.build.is_empty()),
        _ => panic!("unknown generated contract format: {name}"),
    }
}
pub fn matches(rule: &Value, value: &Value) -> bool {
    if let Some(reference) = rule["$ref"].as_str() {
        return matches(
            &crate::contract()["$defs"][reference.rsplit('/').next().unwrap()],
            value,
        );
    }
    if rule.get("const").is_some_and(|v| {
        v != value && !(v.is_number() && value.is_number() && v.as_f64() == value.as_f64())
    }) || rule["enum"]
        .as_array()
        .is_some_and(|choices| !choices.contains(value))
        || rule["oneOf"].as_array().is_some_and(|choices| {
            choices.iter().filter(|child| matches(child, value)).count() != 1
        })
    {
        return false;
    }
    match rule["type"].as_str() {
        Some("null") => value.is_null(),
        Some("boolean") => value.is_boolean(),
        Some("integer" | "number") => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && (rule["type"] != "integer" || n.fract() == 0.0)
                && n >= rule["minimum"].as_f64().unwrap_or(f64::NEG_INFINITY)
                && n <= rule["maximum"].as_f64().unwrap_or(f64::INFINITY)
        }),
        Some("string") => value.as_str().is_some_and(|text| {
            let length = text.chars().count() as u64;
            length >= rule["minLength"].as_u64().unwrap_or(0)
                && length <= rule["maxLength"].as_u64().unwrap_or(u64::MAX)
                && rule["pattern"].as_str().is_none_or(|pattern| {
                    regex::Regex::new(pattern)
                        .expect("generated contract pattern")
                        .is_match(text)
                })
                && rule["format"]
                    .as_str()
                    .is_none_or(|name| format(name, text))
        }),
        Some("array") => value.as_array().is_some_and(|items| {
            items.len() as u64 >= rule["minItems"].as_u64().unwrap_or(0)
                && items.len() as u64 <= rule["maxItems"].as_u64().unwrap_or(u64::MAX)
                && (rule["uniqueItems"] != true
                    || items
                        .iter()
                        .map(Value::to_string)
                        .collect::<HashSet<_>>()
                        .len()
                        == items.len())
                && items.iter().all(|item| matches(&rule["items"], item))
        }),
        Some("object") => value.as_object().is_some_and(|object| {
            rule["required"].as_array().is_none_or(|required| {
                required
                    .iter()
                    .all(|key| object.contains_key(key.as_str().unwrap()))
            }) && (rule["additionalProperties"] != false
                || object
                    .keys()
                    .all(|key| rule["properties"].get(key).is_some()))
                && rule["properties"].as_object().is_none_or(|properties| {
                    properties
                        .iter()
                        .all(|(key, child)| object.get(key).is_none_or(|v| matches(child, v)))
                })
        }),
        _ => true,
    }
}
pub fn normalize(rule: &Value, value: &mut Value) {
    if let Some(reference) = rule["$ref"].as_str() {
        normalize(
            &crate::contract()["$defs"][reference.rsplit('/').next().unwrap()],
            value,
        );
    } else if let Some(choices) = rule["oneOf"].as_array() {
        if let Some(choice) = choices.iter().find(|choice| matches(choice, value)) {
            normalize(choice, value);
        }
    } else if rule["type"] == "integer" || rule["const"].is_number() {
        *value = Value::from(value.as_f64().expect("validated integer") as u64);
    } else if let Some(items) = value.as_array_mut() {
        for item in items {
            normalize(&rule["items"], item);
        }
    } else if let Some(object) = value.as_object_mut() {
        for (key, item) in object {
            if let Some(child) = rule["properties"].get(key) {
                normalize(child, item);
            }
        }
    }
}
pub(crate) fn budget(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    max_nodes: usize,
) -> Result<(), String> {
    *nodes += 1;
    if depth > crate::BUDGET_DEPTH || *nodes > max_nodes {
        return Err("BUDGET_EXCEEDED".into());
    }
    if value
        .as_f64()
        .is_some_and(|n| !n.is_finite() || n.abs() > 9_007_199_254_740_991.0)
    {
        return Err("INVALID_PAYLOAD".into());
    }
    match value {
        Value::Array(items) => {
            for item in items {
                budget(item, depth + 1, nodes, max_nodes)?;
            }
        }
        Value::Object(object) => {
            for item in object.values() {
                budget(item, depth + 1, nodes, max_nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn parse<T: serde::de::DeserializeOwned>(
    raw: &str,
    name: &str,
    bytes: usize,
) -> Result<T, String> {
    if raw.len() > bytes {
        return Err("BUDGET_EXCEEDED".into());
    }
    let mut value: Value = serde_json::from_str(raw).map_err(|e| format!("INVALID_JSON: {e}"))?;
    budget(
        &value,
        1,
        &mut 0,
        if bytes == crate::BUDGET_BYTES {
            crate::BUDGET_NODES
        } else {
            crate::BUDGET_MANIFEST_NODES
        },
    )?;
    let rule = &crate::contract()["$defs"][name];
    if !matches(rule, &value) {
        return Err(format!("INVALID_{}", name.to_uppercase()));
    }
    normalize(rule, &mut value);
    serde_json::from_value(value).map_err(|e| format!("INVALID_{}: {e}", name.to_uppercase()))
}
