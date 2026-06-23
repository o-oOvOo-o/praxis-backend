use super::*;

pub const CUNNING3D_BRIDGE_SCHEMA_V1: &str = "cunning3d.bridge.v1";
pub const CUNNING3D_BRIDGE_EXTENSION_ID: &str = CUNNING3D_BRIDGE_SCHEMA_V1;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum Cunning3dBridgeCommandKind {
    GraphSnapshot,
    CreateNode,
    ConnectNodes,
    SetParameter,
    CookGraph,
    InspectGeometry,
    InspectHeightfield,
    CaptureViewport,
    GetSelection,
    RunDiagnostic,
    OpenNodePanel,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Cunning3dBridgeCallParams {
    pub schema: String,
    pub command: Cunning3dBridgeCommandKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub payload: JsonValue,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum Cunning3dBridgeStatus {
    Ok,
    Failed,
    Unsupported,
    Unavailable,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum Cunning3dBridgeDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Cunning3dBridgeDiagnostic {
    pub severity: Cunning3dBridgeDiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Cunning3dBridgeArtifactHandle {
    pub id: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_length: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Cunning3dBridgeCallResponse {
    pub schema: String,
    pub status: Cunning3dBridgeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub output: JsonValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Cunning3dBridgeDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Cunning3dBridgeArtifactHandle>,
}
