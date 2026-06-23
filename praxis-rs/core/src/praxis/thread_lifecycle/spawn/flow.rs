use std::sync::Arc;

use crate::error::Result as PraxisResult;

use super::super::super::Praxis;
use super::super::PraxisSpawnArgs;
use super::super::PraxisSpawnOk;
use super::super::exec_policy;
use super::super::session_setup;
use super::channels;
use super::loop_spawn;
use super::session_factory;

pub(super) struct SpawnFlow {
    args: PraxisSpawnArgs,
}

impl From<PraxisSpawnArgs> for SpawnFlow {
    fn from(args: PraxisSpawnArgs) -> Self {
        Self { args }
    }
}

impl SpawnFlow {
    pub(super) async fn run(self) -> PraxisResult<PraxisSpawnOk> {
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
        } = self.args;
        let channels = channels::open();

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

        let session = session_factory::build(session_factory::SessionFactoryInput {
            session_configuration,
            llm_runtime_catalog,
            config: Arc::clone(&config),
            auth_manager,
            models_manager,
            exec_policy,
            tx_event: channels.tx_event,
            agent_status_tx: channels.agent_status_tx,
            conversation_history,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
            agent_control,
            agent_os,
        })
        .await?;

        let thread_id = session.conversation_id;
        let session_loop_termination =
            loop_spawn::start(Arc::clone(&session), config, channels.rx_sub);
        let praxis = Praxis {
            tx_sub: channels.tx_sub,
            rx_event: channels.rx_event,
            agent_status: channels.agent_status_rx,
            session,
            session_loop_termination,
        };

        Ok(PraxisSpawnOk { praxis, thread_id })
    }
}
