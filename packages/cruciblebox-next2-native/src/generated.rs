// Generated from contracts/next2/contract.json.
pub const MAX_FRAME_BYTES: usize = 65536;
pub(crate) const WIRE_VERSION: u64 = 4;
pub(crate) const SDK_API_VERSION: u64 = 6;
pub(crate) const REQUEST_TIMEOUT_MS: u64 = 30000;
pub type CapabilitySchema = serde_json::Value;
pub type LegacyServiceMethod = String;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityMethod {
    pub name: LegacyServiceMethod,
    pub params: CapabilitySchema,
    pub result: CapabilitySchema,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ServiceMethod {
    LegacyServiceMethod(LegacyServiceMethod),
    CapabilityMethod(CapabilityMethod),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceExport {
    pub id: String,
    pub version: String,
    pub methods: Vec<ServiceMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interchangeable: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityBinding {
    pub provider_id: String,
    pub provider_version: String,
    pub lifecycle: String,
    pub export: ServiceExport,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}
