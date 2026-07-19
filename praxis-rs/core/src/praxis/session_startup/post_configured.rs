mod background_tasks;
mod initial_history;
mod plugin_services;

use std::collections::HashMap;
use std::sync::Arc;

use async_channel::Sender;
use praxis_config::types::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use tracing::info;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;

pub(super) struct PostConfiguredInput<'a> {
    pub(super) session: Arc<Session>,
    pub(super) config: Arc<Config>,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) mcp_manager: &'a McpManager,
    pub(super) tx_event: Sender<Event>,
    pub(super) auth: Option<&'a OpenAiAccountAuth>,
    pub(super) mcp_servers: HashMap<String, McpServerConfig>,
    pub(super) auth_statuses: HashMap<String, McpAuthStatusEntry>,
    pub(super) initial_history: InitialHistory,
}

pub(super) async fn run(input: PostConfiguredInput<'_>) -> anyhow::Result<()> {
    let PostConfiguredInput {
        session,
        config,
        session_configuration,
        mcp_manager,
        tx_event,
        auth,
        mcp_servers,
        auth_statuses,
        initial_history,
    } = input;

    info!(conversation_id = %session.conversation_id, phase = "plugin_services", "Post-configured startup entering phase");
    plugin_services::start(plugin_services::PluginServicesInput {
        session: &session,
        config: config.as_ref(),
        session_configuration,
        mcp_manager,
        tx_event,
        auth,
        mcp_servers,
        auth_statuses,
    })
    .await?;
    info!(conversation_id = %session.conversation_id, phase = "startup_prewarm", "Post-configured startup entering phase");
    background_tasks::schedule_startup_prewarm(&session, session_configuration).await;
    info!(conversation_id = %session.conversation_id, phase = "initial_history", "Post-configured startup entering phase");
    initial_history::record(&session, initial_history).await;
    info!(conversation_id = %session.conversation_id, phase = "memory_bootstrap", "Post-configured startup entering phase");
    background_tasks::start_memory_bootstrap(&session, config, session_configuration);
    info!(conversation_id = %session.conversation_id, phase = "complete", "Post-configured startup completed");

    Ok(())
}
