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

use super::super::super::super::super::SessionConfiguration;
use super::super::super::post_assembly;

pub(super) struct PostContextProjection {
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

pub(super) fn build(input: PostContextProjection) -> post_assembly::PostAssemblyContext {
    post_assembly::PostAssemblyContext {
        conversation_id: input.conversation_id,
        forked_from_id: input.forked_from_id,
        config: input.config,
        tx_event: input.tx_event,
        session_configuration: input.session_configuration,
        initial_history: input.initial_history,
        history_log_id: input.history_log_id,
        history_entry_count: input.history_entry_count,
        network_proxy: input.network_proxy,
        rollout_path: input.rollout_path,
        post_configured_events: input.post_configured_events,
        auth: input.auth,
        mcp_servers: input.mcp_servers,
        auth_statuses: input.auth_statuses,
        mcp_manager: input.mcp_manager,
    }
}
