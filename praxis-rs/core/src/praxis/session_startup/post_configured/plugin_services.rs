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

use super::super::mcp_startup;
use super::super::skills_watcher;

pub(super) struct PluginServicesInput<'a> {
    pub(super) session: &'a Arc<Session>,
    pub(super) config: &'a Config,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) mcp_manager: &'a McpManager,
    pub(super) tx_event: Sender<Event>,
    pub(super) auth: Option<&'a OpenAiAccountAuth>,
    pub(super) mcp_servers: HashMap<String, McpServerConfig>,
    pub(super) auth_statuses: HashMap<String, McpAuthStatusEntry>,
}

pub(super) async fn start(input: PluginServicesInput<'_>) -> anyhow::Result<()> {
    skills_watcher::start_listener(input.session);
    mcp_startup::start(mcp_startup::McpStartupInput {
        session: input.session,
        config: input.config,
        session_configuration: input.session_configuration,
        mcp_manager: input.mcp_manager,
        tx_event: input.tx_event,
        auth: input.auth,
        mcp_servers: input.mcp_servers,
        auth_statuses: input.auth_statuses,
    })
    .await
}
