use std::sync::Arc;
use std::time::Duration;

use tracing::warn;

use super::super::super::network_proxy;
use super::SessionRuntimePreparation;
use super::SessionRuntimePreparationInput;
use super::agent_os_runtime;
use super::hook_runtime;
use super::session_identity;

pub(in crate::praxis::session_startup::pipeline) async fn prepare(
    input: SessionRuntimePreparationInput<'_>,
) -> anyhow::Result<SessionRuntimePreparation> {
    let identity = input.identity;
    let control = input.control;
    let session_configuration = identity.session_configuration;

    tracing::info!(conversation_id = %identity.conversation_id, phase = "identity", "Session runtime preparation entering phase");
    let session_identity::SessionIdentityRuntime {
        session_telemetry,
        network_proxy_audit_metadata,
        default_shell,
        shell_snapshot_tx,
    } = session_identity::prepare(session_identity::SessionIdentityRuntimeInput {
        conversation_id: identity.conversation_id,
        forked_from_id: identity.forked_from_id,
        initial_history: identity.initial_history,
        state_db_ctx: identity.state_db_ctx,
        config: identity.config,
        auth_manager: identity.auth_manager,
        auth: identity.auth,
        session_configuration: &mut *session_configuration,
        mcp_servers: identity.mcp_servers,
    })
    .await?;

    tracing::info!(conversation_id = %identity.conversation_id, phase = "network", "Session runtime preparation entering phase");
    const NETWORK_PROXY_STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
    let network_config = Arc::clone(identity.config);
    let network_exec_policy = Arc::clone(control.exec_policy);
    let network_runtime = tokio::runtime::Handle::current();
    let mut network_start = tokio::task::spawn_blocking(move || {
        network_runtime.block_on(network_proxy::start(
            network_config.as_ref(),
            network_exec_policy.as_ref(),
            network_proxy_audit_metadata,
        ))
    });
    let network_bootstrap = match tokio::time::timeout(
        NETWORK_PROXY_STARTUP_TIMEOUT,
        &mut network_start,
    )
    .await
    {
        Ok(result) => result??,
        Err(_) => {
            network_start.abort();
            warn!(
                timeout_ms = NETWORK_PROXY_STARTUP_TIMEOUT.as_millis(),
                "Managed network proxy startup timed out; continuing with sandbox policy and approval enforcement"
            );
            network_proxy::without_managed_proxy(identity.config.as_ref())
        }
    };
    let network_proxy::NetworkBootstrap {
        network_proxy: started_network_proxy,
        session_network_proxy,
        network_approval,
        policy_decider_session: network_policy_decider_session,
    } = network_bootstrap;

    tracing::info!(conversation_id = %identity.conversation_id, phase = "hooks", "Session runtime preparation entering phase");
    let hooks = hook_runtime::build(
        identity.config.as_ref(),
        &default_shell,
        control.post_session_configured_events,
    );
    tracing::info!(conversation_id = %identity.conversation_id, phase = "agent_os", "Session runtime preparation entering phase");
    let unified_exec_manager = agent_os_runtime::register_and_attach(
        control.agent_os,
        identity.state_db_ctx,
        identity.conversation_id,
        session_configuration,
        identity.config.background_terminal_max_timeout,
    )
    .await?;
    tracing::info!(conversation_id = %identity.conversation_id, phase = "complete", "Session runtime preparation completed");

    Ok(SessionRuntimePreparation {
        session_telemetry,
        default_shell,
        shell_snapshot_tx,
        started_network_proxy,
        session_network_proxy,
        network_approval,
        network_policy_decider_session,
        hooks,
        unified_exec_manager,
    })
}
