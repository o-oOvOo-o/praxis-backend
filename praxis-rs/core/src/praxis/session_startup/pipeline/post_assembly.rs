use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_channel::Sender;
use praxis_config::types::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;

use crate::config::Config;
use crate::mcp::McpManager;

use super::super::super::Session;
use super::super::super::SessionConfiguration;
mod finish;

pub(super) struct AssembledSession {
    pub(super) session: Arc<Session>,
    pub(super) context: PostAssemblyContext,
}

pub(super) struct PostAssemblyContext {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) config: Arc<Config>,
    pub(super) tx_event: Sender<Event>,
    pub(super) session_configuration: SessionConfiguration,
    pub(super) initial_history: InitialHistory,
    pub(super) history_log_id: u64,
    pub(super) history_entry_count: usize,
    pub(super) network_proxy: Option<SessionNetworkProxyRuntime>,
    pub(super) rollout_path: Option<PathBuf>,
    pub(super) post_configured_events: Vec<Event>,
    pub(super) auth: Option<OpenAiAccountAuth>,
    pub(super) mcp_servers: HashMap<String, McpServerConfig>,
    pub(super) auth_statuses: HashMap<String, McpAuthStatusEntry>,
    pub(super) mcp_manager: Arc<McpManager>,
}
