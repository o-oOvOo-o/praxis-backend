use std::path::PathBuf;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;

use crate::config::Config;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::SessionConfiguration;

pub(super) struct SessionConfiguredInput<'a> {
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

pub(super) fn events(input: SessionConfiguredInput<'_>) -> Vec<Event> {
    let mut events = Vec::with_capacity(1 + input.post_configured_events.len());
    events.push(Event {
        id: INITIAL_SUBMIT_ID.to_owned(),
        msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: input.conversation_id,
            forked_from_id: input.forked_from_id,
            thread_name: input.session_configuration.thread_name.clone(),
            model: input
                .session_configuration
                .collaboration_mode
                .model()
                .to_string(),
            model_provider_id: input.config.model_provider_id.clone(),
            service_tier: input.session_configuration.service_tier,
            approval_policy: input.session_configuration.approval_policy.value(),
            approvals_reviewer: input.session_configuration.approvals_reviewer,
            sandbox_policy: input.session_configuration.sandbox_policy.get().clone(),
            cwd: input.session_configuration.cwd.to_path_buf(),
            reasoning_effort: input
                .session_configuration
                .collaboration_mode
                .reasoning_effort(),
            history_log_id: input.history_log_id,
            history_entry_count: input.history_entry_count,
            initial_messages: input.initial_history.get_event_msgs(),
            network_proxy: input.network_proxy,
            rollout_path: input.rollout_path,
        }),
    });
    events.extend(input.post_configured_events);
    events
}
