use super::pending_interactive_replay::PendingInteractiveReplayState;
use crate::app_command::AppCommand;
use crate::app_event::FeedbackCategory;
use crate::app_gateway_session::ThreadSessionState;
use crate::bottom_pane::FeedbackAudience;
use crate::chatwidget::ThreadInputState;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::ThreadRollbackResponse;
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
    pub(super) buffer: VecDeque<ThreadBufferedEvent>,
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
            buffer: VecDeque::new(),
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

    pub(super) fn rebase_buffer_after_session_refresh(&mut self) {
        self.buffer.retain(Self::event_survives_session_refresh);
    }

    pub(super) fn set_turns(&mut self, turns: Vec<Turn>) {
        self.active_turn_id = turns
            .iter()
            .rev()
            .find(|turn| matches!(turn.status, TurnStatus::InProgress))
            .map(|turn| turn.id.clone());
        self.turns = turns;
    }

    pub(super) fn push_notification(&mut self, notification: ServerNotification) {
        self.pending_interactive_replay
            .note_server_notification(&notification);
        if let Some(session) = self.session.as_mut()
            && let ServerNotification::ThreadModelChanged(notification) = &notification
        {
            session.model_provider_id = notification.model_provider.clone();
            session.model = notification.model.clone();
            session.reasoning_effort = notification.reasoning_effort.clone();
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
                self.active_turn_id = None;
            }
            _ => {}
        }
        self.buffer
            .push_back(ThreadBufferedEvent::Notification(notification));
        if self.buffer.len() > self.capacity
            && let Some(removed) = self.buffer.pop_front()
            && let ThreadBufferedEvent::Request(request) = &removed
        {
            self.pending_interactive_replay
                .note_evicted_server_request(request);
        }
    }

    pub(super) fn push_request(&mut self, request: ServerRequest) {
        self.pending_interactive_replay
            .note_server_request(&request);
        self.buffer.push_back(ThreadBufferedEvent::Request(request));
        if self.buffer.len() > self.capacity
            && let Some(removed) = self.buffer.pop_front()
            && let ThreadBufferedEvent::Request(request) = &removed
        {
            self.pending_interactive_replay
                .note_evicted_server_request(request);
        }
    }

    pub(super) fn push_history_entry_response(&mut self, event: GetHistoryEntryResponseEvent) {
        self.push_buffered_event(ThreadBufferedEvent::HistoryEntryResponse(event));
    }

    pub(super) fn push_feedback_submission(&mut self, event: FeedbackThreadEvent) {
        self.push_buffered_event(ThreadBufferedEvent::FeedbackSubmission(event));
    }

    fn push_buffered_event(&mut self, event: ThreadBufferedEvent) {
        self.buffer.push_back(event);
        if self.buffer.len() > self.capacity
            && let Some(removed) = self.buffer.pop_front()
            && let ThreadBufferedEvent::Request(request) = &removed
        {
            self.pending_interactive_replay
                .note_evicted_server_request(request);
        }
    }

    pub(super) fn apply_thread_rollback(&mut self, response: &ThreadRollbackResponse) {
        self.turns = response.thread.turns.clone();
        self.buffer.clear();
        self.pending_interactive_replay = PendingInteractiveReplayState::default();
        self.active_turn_id = None;
    }

    pub(super) fn snapshot(&self) -> ThreadEventSnapshot {
        ThreadEventSnapshot {
            session: self.session.clone(),
            turns: self.turns.clone(),
            events: self
                .buffer
                .iter()
                .filter(|event| match event {
                    ThreadBufferedEvent::Request(request) => self
                        .pending_interactive_replay
                        .should_replay_snapshot_request(request),
                    ThreadBufferedEvent::Notification(_)
                    | ThreadBufferedEvent::HistoryEntryResponse(_)
                    | ThreadBufferedEvent::FeedbackSubmission(_) => true,
                })
                .cloned()
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

    pub(super) fn clear_active_turn_id(&mut self) {
        self.active_turn_id = None;
    }
}

#[derive(Debug)]
pub(super) struct ThreadEventChannel {
    pub(super) sender: mpsc::Sender<ThreadBufferedEvent>,
    pub(super) receiver: Option<mpsc::Receiver<ThreadBufferedEvent>>,
    pub(super) store: Arc<Mutex<ThreadEventStore>>,
}

impl ThreadEventChannel {
    pub(super) fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: Some(receiver),
            store: Arc::new(Mutex::new(ThreadEventStore::new(capacity))),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn new_with_session(
        capacity: usize,
        session: ThreadSessionState,
        turns: Vec<Turn>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: Some(receiver),
            store: Arc::new(Mutex::new(ThreadEventStore::new_with_session(
                capacity, session, turns,
            ))),
        }
    }
}
