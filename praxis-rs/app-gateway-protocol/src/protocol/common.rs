use std::collections::BTreeMap;

use crate::RequestId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use ts_rs::TS;

mod client_request;
mod fuzzy_file_search;
mod server_message;

pub use client_request::*;
pub use fuzzy_file_search::*;
pub use server_message::*;

#[cfg(test)]
use crate::protocol::api;

pub use praxis_protocol::auth::AuthMode;

/// JSON-RPC application error used when a client rejects a server-initiated request.
pub const SERVER_REQUEST_REJECTED_ERROR_CODE: i64 = -32000;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayClientInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GatewayMode {
    Native,
    Service,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GatewayTransport {
    Native,
    Stdio,
    WebSocket,
    NamedPipe,
    UnixSocket,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema, TS)]
pub struct GatewayMetadata(pub BTreeMap<String, JsonValue>);

impl GatewayMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum HostKind {
    Desktop,
    Editor,
    Cli,
    Service,
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GatewayCapabilityKind {
    HostCommand,
    HostSurface,
    MetraCommand,
    MetraSurface,
    ProductBridge,
    SemanticTree,
    Input,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayCapability {
    pub kind: GatewayCapabilityKind,
    pub version: u32,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostExtensionInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub host_kind: HostKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<GatewayCapability>,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetraBridgeDescriptor {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<MetraSurfaceDescriptor>,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetraSurfaceDescriptor {
    pub id: String,
    pub title: String,
    pub surface_type: String,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MetraSemanticSnapshot {
    pub surface_id: String,
    pub revision: u64,
    pub tree: JsonValue,
    #[serde(default, skip_serializing_if = "GatewayMetadata::is_empty")]
    pub metadata: GatewayMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestEnvelope {
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayResponseEnvelope {
    pub id: RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<GatewayErrorPayload>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEventEnvelope {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

#[cfg(test)]
mod tests;
