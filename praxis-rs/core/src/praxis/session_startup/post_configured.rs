use std::collections::HashMap;
use std::sync::Arc;

use async_channel::Sender;
use praxis_config::types::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::memories;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;

use super::mcp_startup;
use super::skills_watcher;

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
    skills_watcher::start_listener(&input.session);
    mcp_startup::start(
        &input.session,
        input.config.as_ref(),
        input.session_configuration,
        input.mcp_manager,
        input.tx_event,
        input.auth,
        input.mcp_servers,
        input.auth_statuses,
    )
    .await?;
    input
        .session
        .schedule_startup_prewarm(input.session_configuration.base_instructions.clone())
        .await;
    let session_start_source = match &input.initial_history {
        InitialHistory::Resumed(_) => praxis_hooks::SessionStartSource::Resume,
        InitialHistory::New | InitialHistory::Forked(_) => {
            praxis_hooks::SessionStartSource::Startup
        }
    };

    input
        .session
        .record_initial_history(input.initial_history)
        .await;
    {
        let mut state = input.session.state.lock().await;
        state.set_pending_session_start_source(Some(session_start_source));
    }

    memories::start_memories_startup_task(
        &input.session,
        Arc::clone(&input.config),
        &input.session_configuration.session_source,
    );

    Ok(())
}
