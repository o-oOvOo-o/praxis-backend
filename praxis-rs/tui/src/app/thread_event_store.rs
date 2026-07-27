use super::pending_interactive_replay::PendingInteractiveReplayState;
use crate::app_command::AppCommand;
use crate::app_event::FeedbackCategory;
use crate::app_gateway_session::ThreadSessionState;
use crate::bottom_pane::FeedbackAudience;
use crate::chatwidget::ThreadInputState;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadRollbackResponse;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_protocol::protocol::GetHistoryEntryResponseEvent;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub(super) struct ThreadEventSnapshot {
    pub(super) session: Option<ThreadSessionState>,
    pub(super) turns: Vec<Turn>,
    pub(super) status: Option<ThreadStatus>,
    pub(super) control_state: Option<ThreadControlState>,
    pub(super) events: Vec<ThreadBufferedEvent>,
    pub(super) input_state: Option<ThreadInputState>,
}

#[derive(Debug, Clone)]
pub(super) enum ThreadBufferedEvent {
    Notification(ServerNotification),
    Request(ServerRequest),
    HistoryEntryResponse(GetHistoryEntryResponseEvent),
    FeedbackSubmission(FeedbackThreadEvent),
}

#[derive(Debug, Clone)]
pub(super) struct ThreadEventEnvelope {
    pub(super) sequence: u64,
    pub(super) event: ThreadBufferedEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FeedbackThreadEvent {
    pub(super) category: FeedbackCategory,
    pub(super) include_logs: bool,
    pub(super) feedback_audience: FeedbackAudience,
    pub(super) result: Result<String, String>,
}

#[derive(Debug)]
pub(super) struct ThreadEventStore {
    pub(super) session: Option<ThreadSessionState>,
    pub(super) turns: Vec<Turn>,
    pub(super) status: Option<ThreadStatus>,
    pub(super) control_state: Option<ThreadControlState>,
    pub(super) buffer: VecDeque<ThreadEventEnvelope>,
    last_event_sequence: u64,
    pending_interactive_replay: PendingInteractiveReplayState,
    active_turn_id: Option<String>,
    pub(super) input_state: Option<ThreadInputState>,
    pub(super) capacity: usize,
    pub(super) active: bool,
}

impl ThreadEventStore {
    pub(super) fn event_survives_session_refresh(event: &ThreadBufferedEvent) -> bool {
        matches!(
            event,
            ThreadBufferedEvent::Request(_)
                | ThreadBufferedEvent::Notification(ServerNotification::HookStarted(_))
                | ThreadBufferedEvent::Notification(ServerNotification::HookCompleted(_))
                | ThreadBufferedEvent::FeedbackSubmission(_)
        )
    }

    pub(super) fn new(capacity: usize) -> Self {
        Self {
            session: None,
            turns: Vec::new(),
            status: None,
            control_state: None,
            buffer: VecDeque::new(),
            last_event_sequence: 0,
            pending_interactive_replay: PendingInteractiveReplayState::default(),
            active_turn_id: None,
            input_state: None,
            capacity,
            active: false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn new_with_session(
        capacity: usize,
        session: ThreadSessionState,
        turns: Vec<Turn>,
    ) -> Self {
        let mut store = Self::new(capacity);
        store.session = Some(session);
        store.set_turns(turns);
        store
    }

    pub(super) fn set_session(&mut self, session: ThreadSessionState, turns: Vec<Turn>) {
        self.session = Some(session);
        self.set_turns(turns);
    }

    pub(super) fn set_runtime_snapshot(
        &mut self,
        session: ThreadSessionState,
        turns: Vec<Turn>,
        status: ThreadStatus,
        control_state: Option<ThreadControlState>,
    ) {
        self.set_session(session, turns);
        self.set_status(status);
        self.control_state = control_state;
    }

    pub(super) fn set_status(&mut self, status: ThreadStatus) {
        if !status.is_active() {
            self.active_turn_id = None;
        }
        self.status = Some(status);
    }

    pub(super) fn rebase_buffer_after_session_refresh(&mut self) {
        let turns = &self.turns;
        self.buffer.retain(|envelope| {
            Self::event_survives_session_refresh(&envelope.event)
                || Self::event_is_not_covered_by_turn_snapshot(&envelope.event, turns)
        });
    }

    fn event_is_not_covered_by_turn_snapshot(event: &ThreadBufferedEvent, turns: &[Turn]) -> bool {
        let ThreadBufferedEvent::Notification(notification) = event else {
            return false;
        };
        let turn = |turn_id: &str| turns.iter().find(|turn| turn.id == turn_id);
        let turn_is_terminal = |turn_id: &str| {
            turn(turn_id).is_some_and(|turn| {
                matches!(
                    turn.status,
                    TurnStatus::Completed | TurnStatus::Interrupted | TurnStatus::Failed
                )
            })
        };
        let item = |turn_id: &str, item_id: &str| {
            turn(turn_id).and_then(|turn| turn.items.iter().find(|item| item.id() == item_id))
        };

        match notification {
            ServerNotification::TurnStarted(notification) => turn(&notification.turn.id)
                .is_none_or(|snapshot| snapshot.status != notification.turn.status),
            ServerNotification::TurnCompleted(notification) => turn(&notification.turn.id)
                .is_none_or(|snapshot| snapshot.status != notification.turn.status),
            ServerNotification::ItemStarted(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && item(&notification.turn_id, notification.item.id()).is_none()
            }
            ServerNotification::ItemCompleted(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && item(&notification.turn_id, notification.item.id()).is_none()
            }
            ServerNotification::AgentMessageDelta(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && !matches!(
                        item(&notification.turn_id, &notification.item_id),
                        Some(praxis_app_gateway_protocol::ThreadItem::AgentMessage { .. })
                    )
            }
            ServerNotification::PlanDelta(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && !matches!(
                        item(&notification.turn_id, &notification.item_id),
                        Some(praxis_app_gateway_protocol::ThreadItem::Plan { .. })
                    )
            }
            ServerNotification::ReasoningSummaryTextDelta(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && !matches!(
                        item(&notification.turn_id, &notification.item_id),
                        Some(praxis_app_gateway_protocol::ThreadItem::Reasoning { .. })
                    )
            }
            ServerNotification::ReasoningTextDelta(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && !matches!(
                        item(&notification.turn_id, &notification.item_id),
                        Some(praxis_app_gateway_protocol::ThreadItem::Reasoning { .. })
                    )
            }
            ServerNotification::ReasoningSummaryPartAdded(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && !matches!(
                        item(&notification.turn_id, &notification.item_id),
                        Some(praxis_app_gateway_protocol::ThreadItem::Reasoning { .. })
                    )
            }
            ServerNotification::CommandExecutionOutputDelta(notification) => {
                !turn_is_terminal(&notification.turn_id)
                    && !matches!(
                        item(&notification.turn_id, &notification.item_id),
                        Some(praxis_app_gateway_protocol::ThreadItem::CommandExecution {
                            aggregated_output: Some(output),
                            ..
                        }) if !output.is_empty()
                    )
            }
            ServerNotification::TerminalInteraction(_)
            | ServerNotification::FileChangeOutputDelta(_)
            | ServerNotification::TurnDiffUpdated(_)
            | ServerNotification::TurnPlanUpdated(_)
            | ServerNotification::ThreadTokenUsageUpdated(_)
            | ServerNotification::ThreadGoalUpdated(_)
            | ServerNotification::ThreadGoalCleared(_)
            | ServerNotification::ThreadHeartbeatUpdated(_)
            | ServerNotification::ThreadSelfworkUpdated(_)
            | ServerNotification::ItemGuardianApprovalReviewStarted(_)
            | ServerNotification::ItemGuardianApprovalReviewCompleted(_)
            | ServerNotification::ModelRerouted(_)
            | ServerNotification::Error(_) => true,
            _ => false,
        }
    }

    pub(super) fn set_turns(&mut self, turns: Vec<Turn>) {
        self.active_turn_id = turns
            .iter()
            .rev()
            .find(|turn| matches!(turn.status, TurnStatus::InProgress))
            .map(|turn| turn.id.clone());
        self.turns = turns;
    }

    pub(super) fn push_notification(&mut self, notification: ServerNotification) -> u64 {
        self.pending_interactive_replay
            .note_server_notification(&notification);
        if let Some(session) = self.session.as_mut()
            && let ServerNotification::ThreadModelChanged(notification) = &notification
        {
            session.model_provider_id = notification.model_provider.clone();
            session.model = notification.model.clone();
            session.reasoning_effort = notification.reasoning_effort.clone();
        }
        if let Some(session) = self.session.as_mut()
            && let ServerNotification::ThreadPermissionsChanged(notification) = &notification
        {
            session.approval_policy = notification.approval_policy.to_core();
            session.approvals_reviewer = notification.approvals_reviewer.to_core();
            session.sandbox_policy = notification.sandbox_policy.to_core();
        }
        if let ServerNotification::ThreadStatusChanged(notification) = &notification {
            self.set_status(notification.status.clone());
        }
        if let ServerNotification::ThreadControlChanged(notification) = &notification {
            self.control_state = notification.control_state.clone();
        }
        match &notification {
            ServerNotification::TurnStarted(turn) => {
                self.active_turn_id = Some(turn.turn.id.clone());
            }
            ServerNotification::TurnCompleted(turn) => {
                if self.active_turn_id.as_deref() == Some(turn.turn.id.as_str()) {
                    self.active_turn_id = None;
                }
            }
            ServerNotification::ThreadClosed(_) => {
                self.set_status(ThreadStatus::NotLoaded);
                self.control_state = None;
            }
            _ => {}
        }
        self.push_buffered_event(ThreadBufferedEvent::Notification(notification))
    }

    pub(super) fn push_request(&mut self, request: ServerRequest) -> bool {
        if self.buffer.iter().any(|envelope| {
            matches!(
                &envelope.event,
                ThreadBufferedEvent::Request(buffered) if buffered.id() == request.id()
            )
        }) {
            return false;
        }
        self.pending_interactive_replay
            .note_server_request(&request);
        self.push_buffered_event(ThreadBufferedEvent::Request(request));
        true
    }

    pub(super) fn push_history_entry_response(
        &mut self,
        event: GetHistoryEntryResponseEvent,
    ) -> u64 {
        self.push_buffered_event(ThreadBufferedEvent::HistoryEntryResponse(event))
    }

    pub(super) fn push_feedback_submission(&mut self, event: FeedbackThreadEvent) -> u64 {
        self.push_buffered_event(ThreadBufferedEvent::FeedbackSubmission(event))
    }

    fn push_buffered_event(&mut self, event: ThreadBufferedEvent) -> u64 {
        self.last_event_sequence = self.last_event_sequence.saturating_add(1);
        let sequence = self.last_event_sequence;
        self.buffer
            .push_back(ThreadEventEnvelope { sequence, event });
        if self.buffer.len() > self.capacity
            && let Some(removed) = self.buffer.pop_front()
            && let ThreadBufferedEvent::Request(request) = &removed.event
        {
            self.pending_interactive_replay
                .note_evicted_server_request(request);
        }
        sequence
    }

    pub(super) fn apply_thread_rollback(&mut self, response: &ThreadRollbackResponse) {
        self.set_turns(response.thread.turns.clone());
        self.set_status(response.thread.status.clone());
        self.control_state = response.thread.control_state.clone();
        self.buffer.clear();
        self.pending_interactive_replay = PendingInteractiveReplayState::default();
    }

    pub(super) fn snapshot(&self) -> ThreadEventSnapshot {
        ThreadEventSnapshot {
            session: self.session.clone(),
            turns: self.turns.clone(),
            status: self.status.clone(),
            control_state: self.control_state.clone(),
            events: self
                .buffer
                .iter()
                .filter(|envelope| match &envelope.event {
                    ThreadBufferedEvent::Request(request) => self
                        .pending_interactive_replay
                        .should_replay_snapshot_request(request),
                    ThreadBufferedEvent::Notification(_)
                    | ThreadBufferedEvent::HistoryEntryResponse(_)
                    | ThreadBufferedEvent::FeedbackSubmission(_) => true,
                })
                .map(|envelope| envelope.event.clone())
                .collect(),
            input_state: self.input_state.clone(),
        }
    }

    pub(super) fn note_outbound_op<T>(&mut self, op: T)
    where
        T: Into<AppCommand>,
    {
        self.pending_interactive_replay.note_outbound_op(op);
    }

    pub(super) fn op_can_change_pending_replay_state<T>(op: T) -> bool
    where
        T: Into<AppCommand>,
    {
        PendingInteractiveReplayState::op_can_change_state(op)
    }

    pub(super) fn has_pending_thread_approvals(&self) -> bool {
        self.pending_interactive_replay
            .has_pending_thread_approvals()
    }

    pub(super) fn active_turn_id(&self) -> Option<&str> {
        self.active_turn_id.as_deref()
    }

    pub(super) fn last_event_sequence(&self) -> u64 {
        self.last_event_sequence
    }

    pub(super) fn clear_active_turn_id(&mut self) {
        self.active_turn_id = None;
    }
}

#[derive(Debug)]
pub(super) struct ThreadEventChannel {
    pub(super) sender: mpsc::UnboundedSender<ThreadEventEnvelope>,
    pub(super) receiver: Option<mpsc::UnboundedReceiver<ThreadEventEnvelope>>,
    pub(super) store: Arc<Mutex<ThreadEventStore>>,
    pub(super) replay_through_sequence: u64,
}

impl ThreadEventChannel {
    pub(super) fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            sender,
            receiver: Some(receiver),
            store: Arc::new(Mutex::new(ThreadEventStore::new(capacity))),
            replay_through_sequence: 0,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn new_with_session(
        capacity: usize,
        session: ThreadSessionState,
        turns: Vec<Turn>,
    ) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            sender,
            receiver: Some(receiver),
            store: Arc::new(Mutex::new(ThreadEventStore::new_with_session(
                capacity, session, turns,
            ))),
            replay_through_sequence: 0,
        }
    }
}
