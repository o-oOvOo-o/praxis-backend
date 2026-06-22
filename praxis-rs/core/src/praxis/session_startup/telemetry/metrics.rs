use std::collections::HashMap;

use praxis_config::types::McpServerConfig;
use praxis_git_utils::get_git_repo_root;
use praxis_otel::SessionTelemetry;
use praxis_otel::metrics::names::THREAD_STARTED_METRIC;
use praxis_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

pub(super) fn emit_startup_metrics(
    session_telemetry: &SessionTelemetry,
    config: &Config,
    session_configuration: &SessionConfiguration,
    mcp_servers: &HashMap<String, McpServerConfig>,
) {
    config.features.emit_metrics(session_telemetry);
    session_telemetry.counter(
        THREAD_STARTED_METRIC,
        /*inc*/ 1,
        &[(
            "is_git",
            if get_git_repo_root(&session_configuration.cwd).is_some() {
                "true"
            } else {
                "false"
            },
        )],
    );

    session_telemetry.conversation_starts(
        config.model_provider.name.as_str(),
        session_configuration.collaboration_mode.reasoning_effort(),
        config
            .model_reasoning_summary
            .unwrap_or(ReasoningSummaryConfig::Auto),
        config.model_context_window,
        config.model_auto_compact_token_limit,
        config.permissions.approval_policy.value(),
        config.permissions.sandbox_policy.get().clone(),
        mcp_servers.keys().map(String::as_str).collect(),
        config.active_profile.clone(),
    );
}
