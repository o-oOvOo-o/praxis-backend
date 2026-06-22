use std::sync::Arc;

use tracing::debug;

use super::super::Session;
use super::input::SessionStartupInput;
use super::post_configured;
use super::startup_notices;

mod session_assembly;
mod session_configured_emit;
mod session_runtime_prepare;
mod startup_artifacts;

pub(super) async fn run(input: SessionStartupInput) -> anyhow::Result<Arc<Session>> {
    let SessionStartupInput {
        mut session_configuration,
        llm_runtime_catalog,
        config,
        auth_manager,
        models_manager,
        exec_policy,
        tx_event,
        agent_status,
        initial_history,
        session_source,
        environment_manager,
        skills_manager,
        plugins_manager,
        mcp_manager,
        skills_watcher,
        agent_control,
        agent_os,
    } = input;

    debug!(
        "Configuring session: model={}; provider={:?}",
        session_configuration.collaboration_mode.model(),
        session_configuration.provider
    );

    let startup_artifacts::StartupArtifacts {
        conversation_id,
        forked_from_id,
        rollout_recorder,
        state_db_ctx,
        history_log_id,
        history_entry_count,
        auth,
        mcp_servers,
        auth_statuses,
        rollout_path,
    } = startup_artifacts::collect(startup_artifacts::StartupArtifactsInput {
        initial_history: &initial_history,
        session_configuration: &session_configuration,
        session_source,
        config: &config,
        auth_manager: &auth_manager,
        mcp_manager: &mcp_manager,
    })
    .await?;

    let mut post_session_configured_events =
        startup_notices::build_post_configured_events(config.as_ref());

    let auth = auth.as_ref();
    let session_runtime_prepare::SessionRuntimePreparation {
        session_telemetry,
        default_shell,
        shell_snapshot_tx,
        started_network_proxy,
        session_network_proxy,
        network_approval,
        network_policy_decider_session,
        hooks,
        unified_exec_manager,
    } = session_runtime_prepare::prepare(session_runtime_prepare::SessionRuntimePreparationInput {
        conversation_id,
        forked_from_id,
        initial_history: &initial_history,
        state_db_ctx: &state_db_ctx,
        config: &config,
        auth_manager: &auth_manager,
        auth,
        session_configuration: &mut session_configuration,
        mcp_servers: &mcp_servers,
        exec_policy: &exec_policy,
        agent_os: &agent_os,
        post_session_configured_events: &mut post_session_configured_events,
    })
    .await?;

    let sess = session_assembly::build(session_assembly::SessionAssemblyInput {
        conversation_id,
        tx_event: &tx_event,
        agent_status,
        config: &config,
        session_configuration: &session_configuration,
        llm_runtime_catalog,
        auth_manager: &auth_manager,
        models_manager: &models_manager,
        exec_policy,
        skills_manager,
        plugins_manager: &plugins_manager,
        mcp_manager: &mcp_manager,
        skills_watcher,
        agent_control,
        agent_os,
        environment_manager,
        hooks,
        rollout_recorder,
        default_shell,
        shell_snapshot_tx,
        session_telemetry,
        started_network_proxy,
        network_approval,
        state_db_ctx: state_db_ctx.clone(),
        unified_exec_manager,
        network_policy_decider_session,
    })
    .await?;
    session_configured_emit::emit(session_configured_emit::SessionConfiguredEmissionInput {
        session: &sess,
        conversation_id,
        forked_from_id,
        config: config.as_ref(),
        session_configuration: &session_configuration,
        initial_history: &initial_history,
        history_log_id,
        history_entry_count,
        network_proxy: session_network_proxy,
        rollout_path,
        post_configured_events: post_session_configured_events,
    })
    .await;
    post_configured::run(post_configured::PostConfiguredInput {
        session: Arc::clone(&sess),
        config: Arc::clone(&config),
        session_configuration: &session_configuration,
        mcp_manager: mcp_manager.as_ref(),
        tx_event: tx_event.clone(),
        auth,
        mcp_servers,
        auth_statuses,
        initial_history,
    })
    .await?;

    Ok(sess)
}
