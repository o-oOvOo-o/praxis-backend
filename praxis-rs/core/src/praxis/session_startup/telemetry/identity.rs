use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_login::default_client::originator;
use praxis_otel::SessionTelemetry;
use praxis_otel::TelemetryAuthMode;
use praxis_protocol::ThreadId;

use crate::config::Config;
use crate::praxis::SessionConfiguration;
use crate::provider_decision_center::ProviderDecisionCenter;

pub(super) struct StartupTelemetryIdentity {
    pub(super) auth_mode: Option<TelemetryAuthMode>,
    pub(super) account_id: Option<String>,
    pub(super) account_email: Option<String>,
    pub(super) originator: String,
    pub(super) terminal_type: String,
    pub(super) session_model: String,
}

pub(super) fn build(
    auth: Option<&OpenAiAccountAuth>,
    session_configuration: &SessionConfiguration,
) -> StartupTelemetryIdentity {
    StartupTelemetryIdentity {
        auth_mode: auth
            .map(OpenAiAccountAuth::auth_mode)
            .map(TelemetryAuthMode::from),
        account_id: auth.and_then(OpenAiAccountAuth::get_account_id),
        account_email: auth.and_then(OpenAiAccountAuth::get_account_email),
        originator: originator().value,
        terminal_type: session_configuration
            .app_gateway_client_name
            .clone()
            .unwrap_or_else(|| session_configuration.session_source.to_string()),
        session_model: session_configuration.collaboration_mode.model().to_string(),
    }
}

pub(super) fn build_session_telemetry(
    conversation_id: ThreadId,
    config: &Config,
    auth_manager: &Arc<AuthManager>,
    session_configuration: &SessionConfiguration,
    identity: &StartupTelemetryIdentity,
) -> SessionTelemetry {
    let telemetry_auth_manager = ProviderDecisionCenter::provider_auth_manager(
        Some(Arc::clone(auth_manager)),
        &session_configuration.provider,
    );
    let auth_env_telemetry = ProviderDecisionCenter::new(telemetry_auth_manager)
        .auth_env_telemetry(&session_configuration.provider);
    let mut session_telemetry = SessionTelemetry::new(
        conversation_id,
        identity.session_model.as_str(),
        identity.session_model.as_str(),
        identity.account_id.clone(),
        identity.account_email.clone(),
        identity.auth_mode,
        identity.originator.clone(),
        config.otel.log_user_prompt,
        identity.terminal_type.clone(),
        session_configuration.session_source.clone(),
    )
    .with_auth_env(auth_env_telemetry.to_otel_metadata());

    if let Some(service_name) = session_configuration.metrics_service_name.as_deref() {
        session_telemetry = session_telemetry.with_metrics_service_name(service_name);
    }

    session_telemetry
}
