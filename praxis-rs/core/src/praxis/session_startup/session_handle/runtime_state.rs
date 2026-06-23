use std::sync::Arc;

use praxis_protocol::protocol::McpServerRefreshConfig;
use tokio::sync::Mutex;

use crate::goals::GoalRuntimeState;
use crate::guardian::GuardianReviewSessionManager;
use crate::realtime_conversation::RealtimeConversationManager;
use crate::state::ActiveTurn;

pub(super) struct SessionRuntimeState {
    pub(super) pending_mcp_server_refresh_config: Mutex<Option<McpServerRefreshConfig>>,
    pub(super) conversation: Arc<RealtimeConversationManager>,
    pub(super) active_turn: Mutex<Option<ActiveTurn>>,
    pub(super) guardian_review_session: GuardianReviewSessionManager,
    pub(super) goal_runtime: GoalRuntimeState,
}

pub(super) fn build() -> SessionRuntimeState {
    SessionRuntimeState {
        pending_mcp_server_refresh_config: Mutex::new(None),
        conversation: Arc::new(RealtimeConversationManager::new()),
        active_turn: Mutex::new(None),
        guardian_review_session: GuardianReviewSessionManager::default(),
        goal_runtime: GoalRuntimeState::new(),
    }
}
