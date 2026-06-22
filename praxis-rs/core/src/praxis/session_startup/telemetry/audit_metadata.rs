use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_protocol::ThreadId;

use super::identity::StartupTelemetryIdentity;

pub(super) fn build(
    conversation_id: ThreadId,
    identity: &StartupTelemetryIdentity,
) -> NetworkProxyAuditMetadata {
    NetworkProxyAuditMetadata {
        conversation_id: Some(conversation_id.to_string()),
        app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        user_account_id: identity.account_id.clone(),
        auth_mode: identity.auth_mode.map(|mode| mode.to_string()),
        originator: Some(identity.originator.clone()),
        user_email: identity.account_email.clone(),
        terminal_type: Some(identity.terminal_type.clone()),
        model: Some(identity.session_model.clone()),
        slug: Some(identity.session_model.clone()),
    }
}
