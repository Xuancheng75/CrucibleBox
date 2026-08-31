//! Versioned JSON-lines protocol shared by the OCR worker and its host.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_LINE_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRequest {
    pub protocol_version: Option<u32>,
    pub request_id: Option<String>,
    pub task: String,
    pub input: String,
    pub options: Option<OcrOptions>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrOptions {
    pub language: Option<String>,
    pub device: Option<String>,
    pub model_directory: Option<String>,
    pub dictionary_path: Option<String>,
    pub model_profile: Option<String>,
}

impl OcrRequest {
    pub fn request_id(&self) -> &str {
        self.request_id.as_deref().unwrap_or("unknown")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressResponse<'a> {
    pub protocol_version: u32,
    pub request_id: &'a str,
    pub r#type: &'static str,
    pub stage: &'static str,
    pub percent: u8,
    pub message: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub r#type: &'static str,
    pub text: String,
    pub blocks: Vec<TextBlock>,
    pub model: ModelInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub detection_sha256: String,
    pub recognition_sha256: String,
    pub dictionary_sha256: String,
    pub device: &'static str,
    pub model_profile: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBlock {
    pub text: String,
    pub polygon: [[i32; 2]; 4],
    pub bbox: [i32; 4],
    pub confidence: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub r#type: &'static str,
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub fn error(request_id: impl Into<String>, code: &str, message: impl Into<String>) -> Value {
    serde_json::to_value(ErrorResponse {
        protocol_version: PROTOCOL_VERSION,
        request_id: request_id.into(),
        r#type: "error",
        error: message.into(),
        code: code.to_string(),
        details: None,
    })
    .expect("protocol error is serializable")
}

pub fn validate_request(request: &OcrRequest) -> Result<(), Value> {
    if request.protocol_version.unwrap_or(PROTOCOL_VERSION) != PROTOCOL_VERSION {
        return Err(error(
            request.request_id(),
            "unsupported-protocol",
            format!("unsupported protocol version (expected {PROTOCOL_VERSION})"),
        ));
    }
    if request.task != "ocr" {
        return Err(error(
            request.request_id(),
            "unknown-task",
            format!("unknown task: {}", request.task),
        ));
    }
    if let Some(request_id) = request.request_id.as_deref() {
        if request_id.is_empty()
            || request_id.len() > 128
            || !request_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(error(
                request.request_id(),
                "invalid-request-id",
                "requestId must be 1-128 ASCII letters, digits, '_' or '-'",
            ));
        }
    }
    if request.input.trim().is_empty() || request.input.len() > 32 * 1024 {
        return Err(error(
            request.request_id(),
            "invalid-input",
            "input path is empty or too long",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_v1_request_and_camel_case_options() {
        let req: OcrRequest = serde_json::from_str(
            r#"{"protocolVersion":1,"requestId":"r1","task":"ocr","input":"a.png","options":{"modelDirectory":"m","dictionaryPath":"d"}}"#,
        )
        .unwrap();
        assert_eq!(req.request_id(), "r1");
        assert_eq!(
            req.options.as_ref().unwrap().dictionary_path.as_deref(),
            Some("d")
        );
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn rejects_unknown_protocol_and_task() {
        let req: OcrRequest = serde_json::from_str(
            r#"{"protocolVersion":2,"requestId":"r1","task":"parse","input":"a"}"#,
        )
        .unwrap();
        let err = validate_request(&req).unwrap_err();
        assert_eq!(err["code"], "unsupported-protocol");
    }
}
