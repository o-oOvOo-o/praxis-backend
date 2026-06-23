use std::collections::HashMap;
use std::path::PathBuf;

use praxis_config::types::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_protocol::ThreadId;
use praxis_rollout::state_db::StateDbHandle;

use crate::rollout::RolloutRecorder;

pub(in crate::praxis::session_startup::pipeline) struct StartupArtifacts {
    pub(in crate::praxis::session_startup::pipeline) conversation_id: ThreadId,
    pub(in crate::praxis::session_startup::pipeline) forked_from_id: Option<ThreadId>,
    pub(in crate::praxis::session_startup::pipeline) rollout_recorder: Option<RolloutRecorder>,
    pub(in crate::praxis::session_startup::pipeline) state_db_ctx: Option<StateDbHandle>,
    pub(in crate::praxis::session_startup::pipeline) history_log_id: u64,
    pub(in crate::praxis::session_startup::pipeline) history_entry_count: usize,
    pub(in crate::praxis::session_startup::pipeline) auth: Option<OpenAiAccountAuth>,
    pub(in crate::praxis::session_startup::pipeline) mcp_servers: HashMap<String, McpServerConfig>,
    pub(in crate::praxis::session_startup::pipeline) auth_statuses:
        HashMap<String, McpAuthStatusEntry>,
    pub(in crate::praxis::session_startup::pipeline) rollout_path: Option<PathBuf>,
}
