use std::collections::HashMap;
use std::sync::Arc;

use praxis_config::types::McpServerConfig;
use praxis_hooks::Hooks;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;
use praxis_rollout::state_db::StateDbHandle;
use tokio::sync::watch;

use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::config::StartedNetworkProxy;
use crate::exec_policy::ExecPolicyManager;
use crate::praxis::SessionConfiguration;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::tools::network_approval::NetworkApprovalService;
use crate::unified_exec::UnifiedExecProcessManager;

use super::super::agent_os_bootstrap;
use super::super::hooks_bootstrap;
use super::super::network_proxy;
use super::super::shell_bootstrap;
use super::super::startup_notices;
use super::super::telemetry;
use super::super::thread_name_bootstrap;

pub(super) struct SessionRuntimePreparationInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) initial_history: &'a InitialHistory,
    pub(super) state_db_ctx: &'a Option<StateDbHandle>,
    pub(super) config: &'a Arc<Config>,
    pub(super) auth_manager: &'a Arc<AuthManager>,
    pub(super) auth: Option<&'a OpenAiAccountAuth>,
    pub(super) session_configuration: &'a mut SessionConfiguration,
    pub(super) mcp_servers: &'a HashMap<String, McpServerConfig>,
    pub(super) exec_policy: &'a Arc<ExecPolicyManager>,
    pub(super) agent_os: &'a Arc<AgentOs>,
    pub(super) post_session_configured_events: &'a mut Vec<Event>,
}

pub(super) struct SessionRuntimePreparation {
    pub(super) session_telemetry: SessionTelemetry,
    pub(super) default_shell: Shell,
    pub(super) shell_snapshot_tx: watch::Sender<Option<Arc<ShellSnapshot>>>,
    pub(super) started_network_proxy: Option<StartedNetworkProxy>,
    pub(super) session_network_proxy: Option<SessionNetworkProxyRuntime>,
    pub(super) network_approval: Arc<NetworkApprovalService>,
    pub(super) network_policy_decider_session: network_proxy::PolicyDeciderSession,
    pub(super) hooks: Hooks,
    pub(super) unified_exec_manager: Arc<UnifiedExecProcessManager>,
}

pub(super) async fn prepare(
    input: SessionRuntimePreparationInput<'_>,
) -> anyhow::Result<SessionRuntimePreparation> {
    let telemetry::StartupTelemetry {
        session_telemetry,
        network_proxy_audit_metadata,
    } = telemetry::build_startup_telemetry(
        input.conversation_id,
        input.config.as_ref(),
        input.auth_manager,
        input.auth,
        input.session_configuration,
        input.mcp_servers,
    );

    let shell_bootstrap::ShellBootstrap {
        shell: default_shell,
        snapshot_tx: shell_snapshot_tx,
    } = shell_bootstrap::build(
        input.config.as_ref(),
        input.session_configuration,
        input.conversation_id,
        &session_telemetry,
    )?;
    let thread_name = thread_name_bootstrap::resolve_session_thread_name(
        input.conversation_id,
        input.forked_from_id,
        input.initial_history,
        input.state_db_ctx.as_deref(),
        input.config.ephemeral,
    )
    .await;
    input.session_configuration.thread_name = thread_name.clone();
    let network_proxy::NetworkBootstrap {
        network_proxy: started_network_proxy,
        session_network_proxy,
        network_approval,
        policy_decider_session: network_policy_decider_session,
    } = network_proxy::start(
        input.config.as_ref(),
        input.exec_policy.as_ref(),
        network_proxy_audit_metadata,
    )
    .await?;

    let hooks = hooks_bootstrap::build(input.config.as_ref(), &default_shell);
    for warning in hooks.startup_warnings() {
        input
            .post_session_configured_events
            .push(startup_notices::hook_warning_event(warning.clone()));
    }

    agent_os_bootstrap::register_session_thread(
        input.agent_os,
        input.state_db_ctx.clone(),
        input.conversation_id,
        input.session_configuration,
    )
    .await?;

    let unified_exec_manager = Arc::new(UnifiedExecProcessManager::new(
        input.config.background_terminal_max_timeout,
    ));
    agent_os_bootstrap::attach_process_cleaners(input.agent_os, Arc::clone(&unified_exec_manager))
        .await;

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
