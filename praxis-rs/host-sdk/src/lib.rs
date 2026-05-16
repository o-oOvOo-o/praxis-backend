#![forbid(unsafe_code)]

use praxis_app_gateway_protocol::GatewayCapabilityKind;
use praxis_app_gateway_protocol::GatewayMetadata;
use praxis_app_gateway_protocol::HostExtensionInfo;

/// Describes a host extension without leaking product-specific state into Praxis.
pub trait HostExtension: Send + Sync {
    fn info(&self) -> HostExtensionInfo;
}

/// Supplies host commands that App Gateway may reflect to clients.
pub trait HostCommandProvider: HostExtension {
    fn commands(&self) -> Vec<HostCommandDescriptor>;
}

/// Supplies native surfaces that App Gateway may observe or control.
pub trait HostSurfaceProvider: HostExtension {
    fn surfaces(&self) -> Vec<HostSurfaceDescriptor>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostCommandDescriptor {
    pub id: String,
    pub title: String,
    pub capability: GatewayCapabilityKind,
    pub input_schema: Option<serde_json::Value>,
    pub metadata: GatewayMetadata,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostSurfaceDescriptor {
    pub id: String,
    pub title: String,
    pub surface_type: String,
    pub metadata: GatewayMetadata,
}
