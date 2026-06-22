use std::sync::Arc;

use tokio::sync::watch;
use tracing::Instrument;
use tracing::error;
use tracing::info_span;

use crate::agent::AgentStatus;
use crate::error::Result as PraxisResult;
use crate::rollout::map_session_init_error;

use super::super::Praxis;
use super::super::Session;
use super::super::main_agent_loop::main_agent_loop;
use super::PraxisSpawnArgs;
use super::PraxisSpawnOk;
use super::SUBMISSION_CHANNEL_CAPACITY;
use super::exec_policy;
use super::loop_handle::session_loop_termination_from_handle;
use super::session_setup;
use super::trace;

impl Praxis {
    /// Spawn a new [`Praxis`] and initialize the session.
    pub(crate) async fn spawn(mut args: PraxisSpawnArgs) -> PraxisResult<PraxisSpawnOk> {
        args.parent_trace = trace::valid_parent_trace(args.parent_trace);
        let thread_spawn_span = trace::thread_spawn_span(args.parent_trace.as_ref());
        Self::spawn_internal(args)
            .instrument(thread_spawn_span)
            .await
    }

    async fn spawn_internal(args: PraxisSpawnArgs) -> PraxisResult<PraxisSpawnOk> {
        let PraxisSpawnArgs {
            config,
            auth_manager,
            models_manager,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
            conversation_history,
            session_source,
            agent_control,
            agent_os,
            dynamic_tools,
            persist_extended_history,
            metrics_service_name,
            inherited_shell_snapshot,
            user_shell_override,
            inherited_exec_policy,
            parent_trace: _,
        } = args;
        let (tx_sub, rx_sub) = async_channel::bounded(SUBMISSION_CHANNEL_CAPACITY);
        let (tx_event, rx_event) = async_channel::unbounded();

        let session_setup::PreparedSpawnConfig {
            config,
            llm_runtime_catalog,
        } = session_setup::prepare_config(
            config,
            plugins_manager.as_ref(),
            skills_manager.as_ref(),
            &session_source,
        );

        let exec_policy =
            exec_policy::resolve(&config, &session_source, inherited_exec_policy.as_ref()).await?;

        let session_setup::ResolvedSessionConfiguration {
            config,
            session_configuration,
        } = session_setup::build_session_configuration(
            config,
            &llm_runtime_catalog,
            models_manager.as_ref(),
            &conversation_history,
            session_source,
            dynamic_tools,
            metrics_service_name,
            persist_extended_history,
            inherited_shell_snapshot,
            user_shell_override,
        )
        .await;

        let session_source_clone = session_configuration.session_source.clone();
        let (agent_status_tx, agent_status_rx) = watch::channel(AgentStatus::PendingInit);

        let session = Session::new(
            session_configuration,
            llm_runtime_catalog,
            config.clone(),
            auth_manager.clone(),
            models_manager.clone(),
            exec_policy,
            tx_event.clone(),
            agent_status_tx.clone(),
            conversation_history,
            session_source_clone,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager.clone(),
            skills_watcher,
            agent_control,
            agent_os,
        )
        .await
        .map_err(|e| {
            error!("Failed to create session: {e:#}");
            map_session_init_error(&e, &config.praxis_home)
        })?;
        let thread_id = session.conversation_id;

        let session_for_loop = Arc::clone(&session);
        let session_loop_handle = tokio::spawn(async move {
            main_agent_loop(session_for_loop, config, rx_sub)
                .instrument(info_span!("session_loop", thread_id = %thread_id))
                .await;
        });
        let praxis = Praxis {
            tx_sub,
            rx_event,
            agent_status: agent_status_rx,
            session,
            session_loop_termination: session_loop_termination_from_handle(session_loop_handle),
        };

        Ok(PraxisSpawnOk { praxis, thread_id })
    }
}
