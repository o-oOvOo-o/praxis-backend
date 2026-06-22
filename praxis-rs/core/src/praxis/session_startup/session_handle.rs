use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;

use async_channel::Sender;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use tokio::sync::Mutex;
use tokio::sync::watch;

use crate::agent::AgentStatus;
use crate::agent::Mailbox;
use crate::config::Config;
use crate::guardian::GuardianReviewSessionManager;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::praxis::EffectivePermissions;
use crate::praxis::Session;
use crate::praxis::SessionConfiguration;
use crate::realtime_conversation::RealtimeConversationManager;
use crate::state::SessionServices;
use crate::state::SessionState;

pub(super) struct SessionHandleInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) tx_event: Sender<Event>,
    pub(super) agent_status: watch::Sender<AgentStatus>,
    pub(super) config: &'a Config,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) state: SessionState,
    pub(super) services: SessionServices,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
}

pub(super) fn build(input: SessionHandleInput<'_>) -> Arc<Session> {
    let (out_of_band_elicitation_paused, _out_of_band_elicitation_paused_rx) =
        watch::channel(false);
    let (effective_permissions, _effective_permissions_rx) = watch::channel(
        EffectivePermissions::from_session_configuration(input.session_configuration),
    );
    let (mailbox, mailbox_rx) = Mailbox::new();

    Arc::new(Session {
        conversation_id: input.conversation_id,
        tx_event: input.tx_event,
        agent_status: input.agent_status,
        out_of_band_elicitation_paused,
        effective_permissions,
        state: Mutex::new(input.state),
        features: input.config.features.clone(),
        pending_mcp_server_refresh_config: Mutex::new(None),
        conversation: Arc::new(RealtimeConversationManager::new()),
        active_turn: Mutex::new(None),
        mailbox,
        mailbox_rx: Mutex::new(mailbox_rx),
        idle_pending_input: Mutex::new(Vec::new()),
        guardian_review_session: GuardianReviewSessionManager::default(),
        services: input.services,
        goal_runtime: crate::goals::GoalRuntimeState::new(),
        llm_runtime_catalog: input.llm_runtime_catalog,
        next_internal_sub_id: AtomicU64::new(0),
        auto_title_attempted: AtomicBool::new(false),
        auto_summary_in_flight: AtomicBool::new(false),
    })
}
