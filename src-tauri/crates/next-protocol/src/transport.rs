//! Independent wire-3 backend transport. No legacy envelope or global queues.
use crate::{generated, shape, validate_payload, validate_response, validate_typed_request};
use serde_json::Value;
use std::io::{self, Read, Write};

pub fn validate_frame(raw: &str, token: &str) -> Result<Value, &'static str> {
    let mut value = validate_payload(raw)?;
    let contract = crate::contract();
    let kind = value["kind"].as_str().ok_or("INVALID_REQUEST")?.to_owned();
    let schema = &contract["backendTransport"]["frames"][&kind];
    if schema.is_null() || !shape(schema, &value) {
        return Err("INVALID_REQUEST");
    }
    match kind.as_str() {
        "control" => {
            if value["token"] != token {
                return Err("SESSION_DENIED");
            }
            let method = value["method"].as_str().ok_or("INVALID_REQUEST")?;
            if !shape(
                &contract["backendTransport"]["controlParams"][method],
                &value["params"],
            ) {
                return Err("INVALID_REQUEST");
            }
            value["wireVersion"] = Value::from(3);
        }
        "capability" => {
            let request = validate_typed_request(&value["request"].to_string())?;
            if request.session != token {
                return Err("SESSION_DENIED");
            }
            value["request"] = serde_json::to_value(request).map_err(|_| "INVALID_REQUEST")?;
        }
        "result" | "capability-result" => {
            let id = value["response"]["requestId"]
                .as_str()
                .ok_or("INVALID_RESPONSE")?;
            let response = validate_response(&value["response"].to_string(), id)?;
            value["response"] = serde_json::to_value(response).map_err(|_| "INVALID_RESPONSE")?;
        }
        _ => return Err("INVALID_REQUEST"),
    }
    Ok(value)
}
pub fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    let count = reader.read(&mut header[..1])?;
    if count == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..])?;
    let length = u32::from_be_bytes(header) as usize;
    if length > generated::MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "BUDGET_EXCEEDED",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}
pub fn write_frame(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    let raw = value.to_string();
    validate_payload(&raw).map_err(str::to_owned)?;
    writer
        .write_all(&(raw.len() as u32).to_be_bytes())
        .map_err(|e| e.to_string())?;
    writer
        .write_all(raw.as_bytes())
        .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn control_capability_and_budget_fail_closed() {
        let token = "a".repeat(64);
        let frame = json!({"kind":"control","wireVersion":3,"requestId":"c-1","token":token,"method":"call","params":{"method":"load","args":[]}});
        assert!(validate_frame(&frame.to_string(), &token).is_ok());
        assert_eq!(
            validate_frame(&frame.to_string(), &"b".repeat(64)).unwrap_err(),
            "SESSION_DENIED"
        );
        let mut bad = frame.clone();
        bad["owner"] = json!("foreign");
        assert!(validate_frame(&bad.to_string(), &token).is_err());
        bad = frame.clone();
        bad["method"] = json!("legacy.initialize");
        assert!(validate_frame(&bad.to_string(), &token).is_err());
        bad = frame.clone();
        bad["params"]["args"] = json!({});
        assert!(validate_frame(&bad.to_string(), &token).is_err());
        let oversized = ((generated::MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
        assert!(read_frame(&mut oversized.as_slice()).is_err());
        assert!(read_frame(&mut [0, 0, 0, 4, 1].as_slice()).is_err());
    }
}
