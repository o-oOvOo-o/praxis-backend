mod audit_metadata;
mod identity;
mod metrics;

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

pub(super) struct StartupTelemetry {
    pub(super) session_telemetry: SessionTelemetry,
    pub(super) network_proxy_audit_metadata: NetworkProxyAuditMetadata,
}

pub(super) fn build_startup_telemetry(
    conversation_id: ThreadId,
    config: &Config,
    auth_manager: &Arc<AuthManager>,
    auth: Option<&OpenAiAccountAuth>,
    session_configuration: &SessionConfiguration,
    mcp_servers: &HashMap<String, McpServerConfig>,
) -> StartupTelemetry {
    let telemetry_identity = identity::build(auth, session_configuration);
    let session_telemetry = identity::build_session_telemetry(
        conversation_id,
        config,
        auth_manager,
        session_configuration,
        &telemetry_identity,
    );
    let network_proxy_audit_metadata = audit_metadata::build(conversation_id, &telemetry_identity);

    metrics::emit_startup_metrics(
        &session_telemetry,
        config,
        session_configuration,
        mcp_servers,
    );

    StartupTelemetry {
        session_telemetry,
        network_proxy_audit_metadata,
    }
}
