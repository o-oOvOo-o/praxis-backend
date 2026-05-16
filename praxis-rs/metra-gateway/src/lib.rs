#![forbid(unsafe_code)]

use praxis_app_gateway_protocol::GatewayCapability;
use praxis_app_gateway_protocol::GatewayCapabilityKind;
use praxis_app_gateway_protocol::GatewayMetadata;
use praxis_app_gateway_protocol::HostExtensionInfo;
use praxis_app_gateway_protocol::HostKind;
use praxis_app_gateway_protocol::MetraBridgeDescriptor;
use praxis_app_gateway_protocol::MetraSemanticSnapshot;
use praxis_host_sdk::HostExtension;
use praxis_host_sdk::HostSurfaceDescriptor;
use praxis_host_sdk::HostSurfaceProvider;

pub const METRA_EXTENSION_ID: &str = "praxis.metra";

#[derive(Clone, Debug, PartialEq)]
pub struct MetraInvocation {
    pub accepted: bool,
    pub result: Option<serde_json::Value>,
}

/// Bridges App Gateway to a native Metra runtime.
pub trait MetraRuntimeBridge: Send + Sync {
    fn describe(&self) -> MetraBridgeDescriptor;
    fn snapshot(&self, surface_id: &str) -> Option<MetraSemanticSnapshot>;
    fn invoke_command(&self, command_id: &str, args: serde_json::Value) -> MetraInvocation;
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetraGatewayExtension {
    info: HostExtensionInfo,
    bridge: MetraBridgeDescriptor,
}

impl MetraGatewayExtension {
    pub fn new(version: impl Into<String>, bridge: MetraBridgeDescriptor) -> Self {
        let capabilities = vec![
            GatewayCapability {
                kind: GatewayCapabilityKind::MetraSurface,
                version: 1,
                metadata: GatewayMetadata::new(),
            },
            GatewayCapability {
                kind: GatewayCapabilityKind::MetraCommand,
                version: 1,
                metadata: GatewayMetadata::new(),
            },
            GatewayCapability {
                kind: GatewayCapabilityKind::SemanticTree,
                version: 1,
                metadata: GatewayMetadata::new(),
            },
            GatewayCapability {
                kind: GatewayCapabilityKind::Input,
                version: 1,
                metadata: GatewayMetadata::new(),
            },
        ];

        Self {
            info: HostExtensionInfo {
                id: METRA_EXTENSION_ID.to_string(),
                name: Some("Metra".to_string()),
                version: Some(version.into()),
                host_kind: HostKind::Desktop,
                capabilities,
                metadata: GatewayMetadata::new(),
            },
            bridge,
        }
    }

    pub fn bridge(&self) -> &MetraBridgeDescriptor {
        &self.bridge
    }
}

impl HostExtension for MetraGatewayExtension {
    fn info(&self) -> HostExtensionInfo {
        self.info.clone()
    }
}

impl HostSurfaceProvider for MetraGatewayExtension {
    fn surfaces(&self) -> Vec<HostSurfaceDescriptor> {
        self.bridge
            .surfaces
            .iter()
            .map(|surface| HostSurfaceDescriptor {
                id: surface.id.clone(),
                title: surface.title.clone(),
                surface_type: surface.surface_type.clone(),
                metadata: surface.metadata.clone(),
            })
            .collect()
    }
}
