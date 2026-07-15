//! Turn-scoped state and active turn metadata scaffolding.

use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;

use praxis_protocol::dynamic_tools::DynamicToolResponse;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::request_permissions::RequestPermissionsResponse;
use praxis_protocol::request_user_input::RequestUserInputResponse;
use praxis_rmcp_client::ElicitationResponse;
use rmcp::model::RequestId;
use tokio::sync::oneshot;

use crate::praxis::TurnContext;
use crate::tasks::AgentTask;
use praxis_protocol::protocol::NonSteerableTurnKind;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::TokenUsage;

/// Metadata about the currently running turn.
pub(crate) struct ActiveTurn {
    pub(crate) tasks: IndexMap<String, RunningAgentTask>,
    pub(crate) turn_state: Arc<Mutex<TurnState>>,
    pub(crate) pending_input_ready: Arc<Notify>,
}

impl Default for ActiveTurn {
    fn default() -> Self {
        Self {
            tasks: IndexMap::new(),
            turn_state: Arc::new(Mutex::new(TurnState::default())),
            pending_input_ready: Arc::new(Notify::new()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentTaskKind {
    Regular,
    Review,
    Compact,
    Undo,
    UserShell,
    GhostSnapshot,
}

impl AgentTaskKind {
    pub(crate) fn non_steerable_turn_kind(self) -> Option<NonSteerableTurnKind> {
        match self {
            Self::Review => Some(NonSteerableTurnKind::Review),
            Self::Compact => Some(NonSteerableTurnKind::Compact),
            Self::Regular | Self::Undo | Self::UserShell | Self::GhostSnapshot => None,
        }
    }
}

pub(crate) struct RunningAgentTask {
    pub(crate) done: Arc<Notify>,
    pub(crate) kind: AgentTaskKind,
    pub(crate) task: Arc<dyn AgentTask>,
    pub(crate) cancellation_token: CancellationToken,
    pub(crate) handle: AbortHandle,
    pub(crate) turn_context: Arc<TurnContext>,
    abort_on_drop: bool,
    // Timer recorded when the task drops to capture the full turn duration.
    pub(crate) _timer: Option<praxis_otel::Timer>,
}

impl RunningAgentTask {
    pub(crate) fn new(
        done: Arc<Notify>,
        kind: AgentTaskKind,
        task: Arc<dyn AgentTask>,
        cancellation_token: CancellationToken,
        handle: AbortHandle,
        turn_context: Arc<TurnContext>,
        timer: Option<praxis_otel::Timer>,
    ) -> Self {
        Self {
            done,
            kind,
            task,
            cancellation_token,
            handle,
            turn_context,
            abort_on_drop: true,
            _timer: timer,
        }
    }

    pub(crate) fn disarm_abort_on_drop(&mut self) {
        self.abort_on_drop = false;
    }
}

impl Drop for RunningAgentTask {
    fn drop(&mut self) {
        if self.abort_on_drop {
            self.handle.abort();
        }
    }
}

impl ActiveTurn {
    pub(crate) fn add_task(&mut self, task: RunningAgentTask) {
        let sub_id = task.turn_context.sub_id.clone();
        self.tasks.insert(sub_id, task);
    }

    pub(crate) fn remove_task(&mut self, sub_id: &str) -> bool {
        if let Some(mut task) = self.tasks.swap_remove(sub_id) {
            task.disarm_abort_on_drop();
        }
        self.tasks.is_empty()
    }

    pub(crate) fn drain_tasks(&mut self) -> Vec<RunningAgentTask> {
        self.tasks.drain(..).map(|(_, task)| task).collect()
    }
}

/// Mutable state for a single turn.
#[derive(Default)]
pub(crate) struct TurnState {
    pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pending_request_permissions: HashMap<String, oneshot::Sender<RequestPermissionsResponse>>,
    pending_user_input: HashMap<String, oneshot::Sender<RequestUserInputResponse>>,
    pending_elicitations: HashMap<(String, RequestId), oneshot::Sender<ElicitationResponse>>,
    pending_dynamic_tools: HashMap<String, oneshot::Sender<DynamicToolResponse>>,
    pending_input: Vec<ResponseInputItem>,
    pub(crate) tool_calls: u64,
    pub(crate) token_usage_at_turn_start: TokenUsage,
}

impl TurnState {
    pub(crate) fn insert_pending_approval(
        &mut self,
        key: String,
        tx: oneshot::Sender<ReviewDecision>,
    ) -> Option<oneshot::Sender<ReviewDecision>> {
        self.pending_approvals.insert(key, tx)
    }

    pub(crate) fn remove_pending_approval(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<ReviewDecision>> {
        self.pending_approvals.remove(key)
    }

    pub(crate) fn clear_pending(&mut self) {
        self.pending_approvals.clear();
        self.pending_request_permissions.clear();
        self.pending_user_input.clear();
        self.pending_elicitations.clear();
        self.pending_dynamic_tools.clear();
        self.pending_input.clear();
    }

    pub(crate) fn insert_pending_request_permissions(
        &mut self,
        key: String,
        tx: oneshot::Sender<RequestPermissionsResponse>,
    ) -> Option<oneshot::Sender<RequestPermissionsResponse>> {
        self.pending_request_permissions.insert(key, tx)
    }

    pub(crate) fn remove_pending_request_permissions(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<RequestPermissionsResponse>> {
        self.pending_request_permissions.remove(key)
    }

    pub(crate) fn insert_pending_user_input(
        &mut self,
        key: String,
        tx: oneshot::Sender<RequestUserInputResponse>,
    ) -> Option<oneshot::Sender<RequestUserInputResponse>> {
        self.pending_user_input.insert(key, tx)
    }

    pub(crate) fn remove_pending_user_input(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<RequestUserInputResponse>> {
        self.pending_user_input.remove(key)
    }

    pub(crate) fn insert_pending_elicitation(
        &mut self,
        server_name: String,
        request_id: RequestId,
        tx: oneshot::Sender<ElicitationResponse>,
    ) -> Option<oneshot::Sender<ElicitationResponse>> {
        self.pending_elicitations
            .insert((server_name, request_id), tx)
    }

    pub(crate) fn remove_pending_elicitation(
        &mut self,
        server_name: &str,
        request_id: &RequestId,
    ) -> Option<oneshot::Sender<ElicitationResponse>> {
        self.pending_elicitations
            .remove(&(server_name.to_string(), request_id.clone()))
    }

    pub(crate) fn insert_pending_dynamic_tool(
        &mut self,
        key: String,
        tx: oneshot::Sender<DynamicToolResponse>,
    ) -> Option<oneshot::Sender<DynamicToolResponse>> {
        self.pending_dynamic_tools.insert(key, tx)
    }

    pub(crate) fn remove_pending_dynamic_tool(
        &mut self,
        key: &str,
    ) -> Option<oneshot::Sender<DynamicToolResponse>> {
        self.pending_dynamic_tools.remove(key)
    }

    pub(crate) fn push_pending_input(&mut self, input: ResponseInputItem) {
        self.pending_input.push(input);
    }

    pub(crate) fn prepend_pending_input(&mut self, mut input: Vec<ResponseInputItem>) {
        if input.is_empty() {
            return;
        }

        input.append(&mut self.pending_input);
        self.pending_input = input;
    }

    pub(crate) fn take_pending_input(&mut self) -> Vec<ResponseInputItem> {
        if self.pending_input.is_empty() {
            Vec::with_capacity(0)
        } else {
            let mut ret = Vec::new();
            std::mem::swap(&mut ret, &mut self.pending_input);
            ret
        }
    }

    pub(crate) fn has_pending_input(&self) -> bool {
        !self.pending_input.is_empty()
    }
}

impl ActiveTurn {
    /// Clear any pending approvals and input buffered for the current turn.
    pub(crate) async fn clear_pending(&self) {
        let mut ts = self.turn_state.lock().await;
        ts.clear_pending();
    }
}
