use std::collections::HashMap;
use std::sync::Arc;

use async_channel::Sender;
use praxis_config::types::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_protocol::protocol::Event;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;

pub(in crate::praxis::session_startup) struct McpStartupInput<'a> {
    pub(in crate::praxis::session_startup) session: &'a Arc<Session>,
    pub(in crate::praxis::session_startup) config: &'a Config,
    pub(in crate::praxis::session_startup) session_configuration: &'a SessionConfiguration,
    pub(in crate::praxis::session_startup) mcp_manager: &'a McpManager,
    pub(in crate::praxis::session_startup) tx_event: Sender<Event>,
    pub(in crate::praxis::session_startup) auth: Option<&'a OpenAiAccountAuth>,
    pub(in crate::praxis::session_startup) mcp_servers: HashMap<String, McpServerConfig>,
    pub(in crate::praxis::session_startup) auth_statuses: HashMap<String, McpAuthStatusEntry>,
}
