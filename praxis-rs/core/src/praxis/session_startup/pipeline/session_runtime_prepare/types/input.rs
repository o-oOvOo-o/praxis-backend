use std::collections::HashMap;
use std::sync::Arc;

use praxis_config::types::McpServerConfig;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use praxis_rollout::state_db::StateDbHandle;

use crate::agent_os::AgentOs;
use crate::config::Config;
use crate::exec_policy::ExecPolicyManager;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup::pipeline) struct SessionRuntimePreparationInput<'a> {
    pub(in crate::praxis::session_startup::pipeline) identity: SessionRuntimeIdentityInput<'a>,
    pub(in crate::praxis::session_startup::pipeline) control: SessionRuntimeControlInput<'a>,
}

pub(in crate::praxis::session_startup::pipeline) struct SessionRuntimeIdentityInput<'a> {
    pub(in crate::praxis::session_startup::pipeline) conversation_id: ThreadId,
    pub(in crate::praxis::session_startup::pipeline) forked_from_id: Option<ThreadId>,
    pub(in crate::praxis::session_startup::pipeline) initial_history: &'a InitialHistory,
    pub(in crate::praxis::session_startup::pipeline) state_db_ctx: &'a Option<StateDbHandle>,
    pub(in crate::praxis::session_startup::pipeline) config: &'a Arc<Config>,
    pub(in crate::praxis::session_startup::pipeline) auth_manager: &'a Arc<AuthManager>,
    pub(in crate::praxis::session_startup::pipeline) auth: Option<&'a OpenAiAccountAuth>,
    pub(in crate::praxis::session_startup::pipeline) session_configuration:
        &'a mut SessionConfiguration,
    pub(in crate::praxis::session_startup::pipeline) mcp_servers:
        &'a HashMap<String, McpServerConfig>,
}

pub(in crate::praxis::session_startup::pipeline) struct SessionRuntimeControlInput<'a> {
    pub(in crate::praxis::session_startup::pipeline) exec_policy: &'a Arc<ExecPolicyManager>,
    pub(in crate::praxis::session_startup::pipeline) agent_os: &'a Arc<AgentOs>,
    pub(in crate::praxis::session_startup::pipeline) post_session_configured_events:
        &'a mut Vec<Event>,
}
