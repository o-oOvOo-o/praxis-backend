use std::sync::Arc;

use crate::state::SessionState;

use super::super::super::services_bootstrap;
use super::handle_seed::SessionHandleSeed;
use super::input::SessionAssemblyInput;
use super::parts::SessionAssemblyParts;

impl<'a> SessionAssemblyInput<'a> {
    pub(super) fn into_assembly_parts(self) -> SessionAssemblyParts<'a> {
        let Self {
            handle,
            managers,
            runtime,
        } = self;
        let state = SessionState::new(handle.session_configuration.clone());
        let services_input = services_bootstrap::ServicesBootstrapInput {
            session: services_bootstrap::ServiceSessionSpec {
                config: Arc::clone(handle.config),
                conversation_id: handle.conversation_id.clone(),
                session_configuration: handle.session_configuration.clone(),
            },
            managers: services_bootstrap::ServiceManagerSet {
                auth_manager: Arc::clone(managers.auth_manager),
                models_manager: Arc::clone(managers.models_manager),
                skills_manager: managers.skills_manager,
                plugins_manager: Arc::clone(managers.plugins_manager),
                mcp_manager: Arc::clone(managers.mcp_manager),
                skills_watcher: managers.skills_watcher,
                agent_control: managers.agent_control,
                agent_os: managers.agent_os,
                environment_manager: managers.environment_manager,
            },
            runtime: services_bootstrap::ServiceRuntimeArtifacts {
                exec_policy: runtime.exec_policy,
                hooks: runtime.hooks,
                rollout_recorder: runtime.rollout_recorder,
                default_shell: runtime.default_shell,
                shell_snapshot_tx: runtime.shell_snapshot_tx,
                session_telemetry: runtime.session_telemetry,
                started_network_proxy: runtime.started_network_proxy,
                network_approval: runtime.network_approval,
                state_db_ctx: runtime.state_db_ctx,
                unified_exec_manager: runtime.unified_exec_manager,
            },
        };
        let handle_seed = SessionHandleSeed {
            conversation_id: handle.conversation_id,
            tx_event: handle.tx_event,
            agent_status: handle.agent_status,
            config: handle.config,
            session_configuration: handle.session_configuration,
            llm_runtime_catalog: handle.llm_runtime_catalog,
            network_policy_decider_session: handle.network_policy_decider_session,
        };
        SessionAssemblyParts {
            state,
            services_input,
            handle_seed,
        }
    }
}
