use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;

use async_channel::Sender;
use praxis_protocol::ThreadId;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::McpServerRefreshConfig;
use praxis_system_plugin_approval_control::PermissionController;
use tokio::sync::Mutex;
use tokio::sync::watch;

use crate::agent::AgentStatus;
use crate::agent::Mailbox;
use crate::agent::MailboxReceiver;
use crate::config::ManagedFeatures;
use crate::guardian::GuardianReviewSessionManager;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::realtime_conversation::RealtimeConversationManager;
use crate::state::ActiveTurn;
use crate::state::SessionServices;
use crate::state::SessionState;

/// Long-lived state and service handle for one loaded agent thread.
pub(crate) struct Session {
    pub(crate) conversation_id: ThreadId,
    pub(super) tx_event: Sender<Event>,
    pub(super) agent_status: watch::Sender<AgentStatus>,
    pub(super) out_of_band_elicitation_paused: watch::Sender<bool>,
    pub(super) permission_controller: PermissionController,
    pub(super) state: Mutex<SessionState>,
    /// The set of enabled features should be invariant for the lifetime of the session.
    pub(super) features: ManagedFeatures,
    pub(super) pending_mcp_server_refresh_config: Mutex<Option<McpServerRefreshConfig>>,
    pub(crate) conversation: Arc<RealtimeConversationManager>,
    pub(crate) active_turn: Mutex<Option<ActiveTurn>>,
    pub(super) mailbox: Mailbox,
    pub(super) mailbox_rx: Mutex<MailboxReceiver>,
    pub(super) idle_pending_input: Mutex<Vec<ResponseInputItem>>,
    pub(crate) guardian_review_session: GuardianReviewSessionManager,
    pub(crate) services: SessionServices,
    pub(crate) goal_runtime: crate::goals::GoalRuntimeState,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
    pub(super) next_internal_sub_id: AtomicU64,
    /// Guards one-shot auto-title generation so it runs at most once per session.
    pub(crate) auto_title_attempted: AtomicBool,
    /// Avoids overlapping auto-summary generations for the same thread.
    pub(crate) auto_summary_in_flight: AtomicBool,
}
