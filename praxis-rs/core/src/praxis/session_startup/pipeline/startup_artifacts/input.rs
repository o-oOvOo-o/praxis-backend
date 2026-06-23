use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup::pipeline) struct StartupArtifactsInput<'a> {
    pub(in crate::praxis::session_startup::pipeline) initial_history: &'a InitialHistory,
    pub(in crate::praxis::session_startup::pipeline) session_configuration:
        &'a SessionConfiguration,
    pub(in crate::praxis::session_startup::pipeline) session_source: SessionSource,
    pub(in crate::praxis::session_startup::pipeline) config: &'a Arc<Config>,
    pub(in crate::praxis::session_startup::pipeline) auth_manager: &'a Arc<AuthManager>,
    pub(in crate::praxis::session_startup::pipeline) mcp_manager: &'a Arc<McpManager>,
}
