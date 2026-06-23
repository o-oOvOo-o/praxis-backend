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

    let network_proxy::NetworkBootstrap {
        network_proxy: started_network_proxy,
        session_network_proxy,
        network_approval,
        policy_decider_session: network_policy_decider_session,
    } = network_proxy::start(
        identity.config.as_ref(),
        control.exec_policy.as_ref(),
        network_proxy_audit_metadata,
    )
    .await?;

    let hooks = hook_runtime::build(
        identity.config.as_ref(),
        &default_shell,
        control.post_session_configured_events,
    );
    let unified_exec_manager = agent_os_runtime::register_and_attach(
        control.agent_os,
        identity.state_db_ctx,
        identity.conversation_id,
        session_configuration,
        identity.config.background_terminal_max_timeout,
    )
    .await?;

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
