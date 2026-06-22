use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;

use crate::config::Config;

use super::super::super::Session;
use super::super::super::SessionConfiguration;
use super::super::session_configured;

pub(super) struct SessionConfiguredEmissionInput<'a> {
    pub(super) session: &'a Arc<Session>,
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) config: &'a Config,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) initial_history: &'a InitialHistory,
    pub(super) history_log_id: u64,
    pub(super) history_entry_count: usize,
    pub(super) network_proxy: Option<SessionNetworkProxyRuntime>,
    pub(super) rollout_path: Option<PathBuf>,
    pub(super) post_configured_events: Vec<Event>,
}

pub(super) async fn emit(input: SessionConfiguredEmissionInput<'_>) {
    for event in session_configured::events(session_configured::SessionConfiguredInput {
        conversation_id: input.conversation_id,
        forked_from_id: input.forked_from_id,
        config: input.config,
        session_configuration: input.session_configuration,
        initial_history: input.initial_history,
        history_log_id: input.history_log_id,
        history_entry_count: input.history_entry_count,
        network_proxy: input.network_proxy,
        rollout_path: input.rollout_path,
        post_configured_events: input.post_configured_events,
    }) {
        input.session.send_event_raw(event).await;
    }
}
