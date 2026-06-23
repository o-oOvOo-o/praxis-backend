use std::collections::HashMap;
use std::sync::Arc;

use praxis_config::types::McpServerConfig;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

use super::super::super::super::telemetry;

pub(super) struct TelemetryPhaseInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) config: &'a Arc<Config>,
    pub(super) auth_manager: &'a Arc<AuthManager>,
    pub(super) auth: Option<&'a OpenAiAccountAuth>,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) mcp_servers: &'a HashMap<String, McpServerConfig>,
}

pub(super) struct SessionTelemetryRuntime {
    pub(super) session_telemetry: SessionTelemetry,
    pub(super) network_proxy_audit_metadata: NetworkProxyAuditMetadata,
}

pub(super) fn build(input: TelemetryPhaseInput<'_>) -> SessionTelemetryRuntime {
    let telemetry::StartupTelemetry {
        session_telemetry,
        network_proxy_audit_metadata,
    } = telemetry::build_startup_telemetry(
        input.conversation_id,
        input.config.as_ref(),
        input.auth_manager,
        input.auth,
        input.session_configuration,
        input.mcp_servers,
    );

    SessionTelemetryRuntime {
        session_telemetry,
        network_proxy_audit_metadata,
    }
}
