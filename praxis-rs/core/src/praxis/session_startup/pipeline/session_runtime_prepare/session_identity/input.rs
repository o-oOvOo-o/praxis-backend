use std::collections::HashMap;
use std::sync::Arc;

use praxis_config::types::McpServerConfig;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_rollout::state_db::StateDbHandle;

use crate::config::Config;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) struct SessionIdentityRuntimeInput<
    'a,
> {
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) conversation_id:
        ThreadId,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) forked_from_id:
        Option<ThreadId>,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) initial_history:
        &'a InitialHistory,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) state_db_ctx:
        &'a Option<StateDbHandle>,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) config:
        &'a Arc<Config>,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) auth_manager:
        &'a Arc<AuthManager>,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) auth:
        Option<&'a OpenAiAccountAuth>,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) session_configuration:
        &'a mut SessionConfiguration,
    pub(in crate::praxis::session_startup::pipeline::session_runtime_prepare) mcp_servers:
        &'a HashMap<String, McpServerConfig>,
}
