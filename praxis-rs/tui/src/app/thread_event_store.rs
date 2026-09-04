use super::pending_interactive_replay::PendingInteractiveReplayState;
use crate::app_command::AppCommand;
use crate::app_event::FeedbackCategory;
use crate::app_gateway_session::ThreadSessionState;
use crate::bottom_pane::FeedbackAudience;
use crate::chatwidget::ThreadInputState;
use crate::thread_replay_policy::compact_visible_replay_turns;
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
use std::sync::Mutex;
use std::sync::MutexGuard;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
enum LatestProjectionEventSlot {
    TokenUsage,
    Goal,
    Heartbeat,
    Selfwork,
    TurnDiff,
    Status,
    Control,
}

const LATEST_PROJECTION_EVENT_SLOTS: usize = LatestProjectionEventSlot::Control as usize + 1;
const CACHED_VIEW_DELTA_EVENT_LIMIT: usize = 256;
const STALE_LIVE_QUEUE_ASYNC_DROP_THRESHOLD: usize = 256;

#[derive(Debug)]
struct CachedViewDeltaLog {
    base_revision: u64,
    events: VecDeque<ThreadEventEnvelope>,
    overflowed: bool,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct ThreadEventSnapshot {
    pub(super) session: Option<ThreadSessionState>,
    pub(super) turns: Vec<Turn>,
    pub(super) status: Option<ThreadStatus>,
    pub(super) control_state: Option<ThreadControlState>,
    pub(super) token_usage: Option<praxis_app_gateway_protocol::ThreadTokenUsage>,
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
    pub(super) turns: Arc<Vec<Turn>>,
    pub(super) status: Option<ThreadStatus>,
    pub(super) control_state: Option<ThreadControlState>,
    pub(super) buffer: VecDeque<ThreadEventEnvelope>,
    latest_projection_events: [Option<ThreadEventEnvelope>; LATEST_PROJECTION_EVENT_SLOTS],
    last_event_sequence: u64,
    view_revision: u64,
    cached_view_delta: Option<CachedViewDeltaLog>,
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
            turns: Arc::new(Vec::new()),
            status: None,
            control_state: None,
            buffer: VecDeque::new(),
            latest_projection_events: std::array::from_fn(|_| None),
            last_event_sequence: 0,
            view_revision: 0,
            cached_view_delta: None,
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
        self.clear_cached_view_delta();
        self.session = Some(session);
        self.rebase_buffer_after_turn_snapshot(&turns);
        self.set_turns(turns);
        self.bump_view_revision();
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

    fn rebase_buffer_after_turn_snapshot(&mut self, turns: &[Turn]) {
        self.buffer.retain(|envelope| {
            Self::event_survives_session_refresh(&envelope.event)
                || Self::event_is_not_covered_by_turn_snapshot(&envelope.event, turns)
        });
    }

    #[cfg(test)]
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
        self.turns = Arc::new(compact_visible_replay_turns(turns));
    }

    fn upsert_turn_snapshot(&mut self, turn: Turn) {
        let turns = Arc::make_mut(&mut self.turns);
        if let Some(existing) = turns.iter_mut().find(|existing| existing.id == turn.id) {
            *existing = turn;
        } else {
            turns.push(turn);
        }
    }

    pub(super) fn push_notification(&mut self, notification: ServerNotification) -> u64 {
        let completed_turn = match &notification {
            ServerNotification::TurnCompleted(notification) => Some(notification.turn.clone()),
            _ => None,
        };
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
        let sequence = self.push_buffered_event(ThreadBufferedEvent::Notification(notification));
        if let Some(turn) = completed_turn {
            self.upsert_turn_snapshot(turn);
            let turns = std::mem::take(Arc::make_mut(&mut self.turns));
            self.rebase_buffer_after_turn_snapshot(&turns);
            self.turns = Arc::new(compact_visible_replay_turns(turns));
        }
        sequence
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
        if event_requires_view_replay(&event) {
            self.bump_view_revision();
        }
        let sequence = self.last_event_sequence;
        self.record_cached_view_delta(sequence, &event);
        if let Some(slot) = latest_projection_event_slot(&event) {
            self.latest_projection_events[slot as usize] =
                Some(ThreadEventEnvelope { sequence, event });
            return sequence;
        }
        if let Some(buffered) = self.buffer.back_mut()
            && merge_adjacent_stream_event(&mut buffered.event, &event)
        {
            buffered.sequence = sequence;
            return sequence;
        }
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
        self.clear_cached_view_delta();
        self.set_turns(response.thread.turns.clone());
        self.set_status(response.thread.status.clone());
        self.control_state = response.thread.control_state.clone();
        self.buffer.clear();
        self.latest_projection_events.fill_with(|| None);
        self.pending_interactive_replay = PendingInteractiveReplayState::default();
        self.bump_view_revision();
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> ThreadEventSnapshot {
        ThreadEventSnapshot {
            session: self.session.clone(),
            turns: self.turns.as_ref().clone(),
            status: self.status.clone(),
            control_state: self.control_state.clone(),
            token_usage: self.latest_token_usage(),
            events: self.snapshot_events(),
            input_state: self.input_state.clone(),
        }
    }

    pub(super) fn note_outbound_op<T>(&mut self, op: T)
    where
        T: Into<AppCommand>,
    {
        self.clear_cached_view_delta();
        self.pending_interactive_replay.note_outbound_op(op);
        self.bump_view_revision();
    }

    pub(super) fn begin_cached_view_delta(&mut self, base_revision: u64) {
        self.cached_view_delta =
            (self.view_revision == base_revision).then(|| CachedViewDeltaLog {
                base_revision,
                events: VecDeque::new(),
                overflowed: false,
            });
    }

    pub(super) fn clear_cached_view_delta(&mut self) {
        self.cached_view_delta = None;
    }

    pub(super) fn take_cached_view_delta(
        &mut self,
        base_revision: u64,
    ) -> Option<VecDeque<ThreadEventEnvelope>> {
        let delta = self.cached_view_delta.take()?;
        (delta.base_revision == base_revision && !delta.overflowed).then_some(delta.events)
    }

    fn record_cached_view_delta(&mut self, sequence: u64, event: &ThreadBufferedEvent) {
        if !event_requires_view_replay(event) {
            return;
        }
        let Some(delta) = self.cached_view_delta.as_mut() else {
            return;
        };
        if delta.overflowed {
            return;
        }
        if let Some(buffered) = delta.events.back_mut()
            && merge_adjacent_stream_event(&mut buffered.event, event)
        {
            buffered.sequence = sequence;
            return;
        }
        if delta.events.len() == CACHED_VIEW_DELTA_EVENT_LIMIT {
            delta.events.clear();
            delta.overflowed = true;
            return;
        }
        delta.events.push_back(ThreadEventEnvelope {
            sequence,
            event: event.clone(),
        });
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

    pub(super) fn view_revision(&self) -> u64 {
        self.view_revision
    }

    pub(super) fn latest_token_usage(
        &self,
    ) -> Option<praxis_app_gateway_protocol::ThreadTokenUsage> {
        let envelope = self.latest_projection_events
            [LatestProjectionEventSlot::TokenUsage as usize]
            .as_ref()?;
        let ThreadBufferedEvent::Notification(ServerNotification::ThreadTokenUsageUpdated(
            notification,
        )) = &envelope.event
        else {
            return None;
        };
        Some(notification.token_usage.clone())
    }

    pub(super) fn latest_goal_notification(&self) -> Option<ServerNotification> {
        let envelope =
            self.latest_projection_events[LatestProjectionEventSlot::Goal as usize].as_ref()?;
        let ThreadBufferedEvent::Notification(notification) = &envelope.event else {
            return None;
        };
        matches!(
            notification,
            ServerNotification::ThreadGoalUpdated(_) | ServerNotification::ThreadGoalCleared(_)
        )
        .then(|| notification.clone())
    }

    fn bump_view_revision(&mut self) {
        self.view_revision = self.view_revision.wrapping_add(1);
    }

    #[cfg(test)]
    fn snapshot_events(&self) -> Vec<ThreadBufferedEvent> {
        self.snapshot_event_refs().into_iter().cloned().collect()
    }

    pub(super) fn snapshot_event_refs(&self) -> Vec<&ThreadBufferedEvent> {
        let mut latest = self
            .latest_projection_events
            .iter()
            .flatten()
            .collect::<Vec<_>>();
        latest.sort_unstable_by_key(|envelope| envelope.sequence);

        let mut buffered = self.buffer.iter().peekable();
        let mut latest = latest.into_iter().peekable();
        let mut events = Vec::with_capacity(self.buffer.len() + LATEST_PROJECTION_EVENT_SLOTS);
        loop {
            let envelope = match (buffered.peek(), latest.peek()) {
                (Some(buffered_event), Some(projected))
                    if buffered_event.sequence <= projected.sequence =>
                {
                    buffered.next()
                }
                (Some(_), Some(_)) => latest.next(),
                (Some(_), None) => buffered.next(),
                (None, Some(_)) => latest.next(),
                (None, None) => break,
            }
            .expect("snapshot event iterator should contain the selected event");
            if self.snapshot_event_should_replay(&envelope.event) {
                events.push(&envelope.event);
            }
        }
        events
    }

    fn snapshot_event_should_replay(&self, event: &ThreadBufferedEvent) -> bool {
        match event {
            ThreadBufferedEvent::Request(request) => self
                .pending_interactive_replay
                .should_replay_snapshot_request(request),
            ThreadBufferedEvent::Notification(
                ServerNotification::ThreadStatusChanged(_)
                | ServerNotification::ThreadControlChanged(_)
                | ServerNotification::ThreadTokenUsageUpdated(_)
                | ServerNotification::ThreadHeartbeatUpdated(_)
                | ServerNotification::TurnDiffUpdated(_),
            ) => false,
            ThreadBufferedEvent::Notification(_)
            | ThreadBufferedEvent::HistoryEntryResponse(_)
            | ThreadBufferedEvent::FeedbackSubmission(_) => true,
        }
    }

    pub(super) fn clear_active_turn_id(&mut self) {
        self.active_turn_id = None;
    }
}

fn event_requires_view_replay(event: &ThreadBufferedEvent) -> bool {
    !matches!(
        event,
        ThreadBufferedEvent::Notification(
            ServerNotification::ThreadStatusChanged(_)
                | ServerNotification::ThreadControlChanged(_)
                // Usage is a latest-value status projection applied directly to cached views.
                | ServerNotification::ThreadTokenUsageUpdated(_)
                // Goal state is a latest-value work-panel projection; completion bookkeeping
                // still runs through the original notification handler on cache activation.
                | ServerNotification::ThreadGoalUpdated(_)
                | ServerNotification::ThreadGoalCleared(_)
                // Heartbeats are routing/liveness metadata and have no ChatWidget effect.
                | ServerNotification::ThreadHeartbeatUpdated(_)
                // The diff notification only asks the active view to refresh its status line;
                // switching views already performs that refresh.
                | ServerNotification::TurnDiffUpdated(_)
        )
    )
}

fn latest_projection_event_slot(event: &ThreadBufferedEvent) -> Option<LatestProjectionEventSlot> {
    let ThreadBufferedEvent::Notification(notification) = event else {
        return None;
    };
    match notification {
        ServerNotification::ThreadTokenUsageUpdated(_) => {
            Some(LatestProjectionEventSlot::TokenUsage)
        }
        ServerNotification::ThreadGoalUpdated(_) | ServerNotification::ThreadGoalCleared(_) => {
            Some(LatestProjectionEventSlot::Goal)
        }
        ServerNotification::ThreadHeartbeatUpdated(_) => Some(LatestProjectionEventSlot::Heartbeat),
        ServerNotification::ThreadSelfworkUpdated(_) => Some(LatestProjectionEventSlot::Selfwork),
        ServerNotification::TurnDiffUpdated(_) => Some(LatestProjectionEventSlot::TurnDiff),
        ServerNotification::ThreadStatusChanged(_) => Some(LatestProjectionEventSlot::Status),
        ServerNotification::ThreadControlChanged(_) => Some(LatestProjectionEventSlot::Control),
        _ => None,
    }
}

fn merge_adjacent_stream_event(
    buffered: &mut ThreadBufferedEvent,
    incoming: &ThreadBufferedEvent,
) -> bool {
    let (ThreadBufferedEvent::Notification(buffered), ThreadBufferedEvent::Notification(incoming)) =
        (buffered, incoming)
    else {
        return false;
    };

    match (buffered, incoming) {
        (
            ServerNotification::AgentMessageDelta(buffered),
            ServerNotification::AgentMessageDelta(incoming),
        ) if buffered.thread_id == incoming.thread_id
            && buffered.turn_id == incoming.turn_id
            && buffered.item_id == incoming.item_id =>
        {
            buffered.delta.push_str(&incoming.delta);
            true
        }
        (ServerNotification::PlanDelta(buffered), ServerNotification::PlanDelta(incoming))
            if buffered.thread_id == incoming.thread_id
                && buffered.turn_id == incoming.turn_id
                && buffered.item_id == incoming.item_id =>
        {
            buffered.delta.push_str(&incoming.delta);
            true
        }
        (
            ServerNotification::ReasoningSummaryTextDelta(buffered),
            ServerNotification::ReasoningSummaryTextDelta(incoming),
        ) if buffered.thread_id == incoming.thread_id
            && buffered.turn_id == incoming.turn_id
            && buffered.item_id == incoming.item_id
            && buffered.summary_index == incoming.summary_index =>
        {
            buffered.delta.push_str(&incoming.delta);
            true
        }
        (
            ServerNotification::ReasoningTextDelta(buffered),
            ServerNotification::ReasoningTextDelta(incoming),
        ) if buffered.thread_id == incoming.thread_id
            && buffered.turn_id == incoming.turn_id
            && buffered.item_id == incoming.item_id
            && buffered.content_index == incoming.content_index =>
        {
            buffered.delta.push_str(&incoming.delta);
            true
        }
        (
            ServerNotification::CommandExecutionOutputDelta(buffered),
            ServerNotification::CommandExecutionOutputDelta(incoming),
        ) if buffered.thread_id == incoming.thread_id
            && buffered.turn_id == incoming.turn_id
            && buffered.item_id == incoming.item_id =>
        {
            buffered.delta.push_str(&incoming.delta);
            true
        }
        (
            ServerNotification::FileChangeOutputDelta(buffered),
            ServerNotification::FileChangeOutputDelta(incoming),
        ) if buffered.thread_id == incoming.thread_id
            && buffered.turn_id == incoming.turn_id
            && buffered.item_id == incoming.item_id =>
        {
            buffered.delta.push_str(&incoming.delta);
            true
        }
        _ => false,
    }
}

#[derive(Debug)]
pub(super) struct ThreadEventChannel {
    pub(super) sender: mpsc::UnboundedSender<ThreadEventEnvelope>,
    pub(super) receiver: Option<mpsc::UnboundedReceiver<ThreadEventEnvelope>>,
    pub(super) store: Arc<Mutex<ThreadEventStore>>,
    pub(super) replay_through_sequence: u64,
    pub(super) replay_view_revision: u64,
}

pub(super) fn lock_thread_event_store(
    store: &Mutex<ThreadEventStore>,
) -> MutexGuard<'_, ThreadEventStore> {
    store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl ThreadEventChannel {
    pub(super) fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            sender,
            receiver: Some(receiver),
            store: Arc::new(Mutex::new(ThreadEventStore::new(capacity))),
            replay_through_sequence: 0,
            replay_view_revision: 0,
        }
    }

    /// Starts a new live-delivery epoch while activation establishes its replay marker.
    ///
    /// Every envelope reaches `ThreadEventStore` before it is copied into the live queue, so an
    /// inactive receiver only contains events already represented by replay or a cached-view
    /// delta. Reusing that receiver would make activation walk those duplicate envelopes merely
    /// to reject their old sequence numbers. Replace it instead and release a potentially large
    /// stale queue away from the UI task.
    pub(super) fn take_rebased_receiver(
        &mut self,
    ) -> Option<mpsc::UnboundedReceiver<ThreadEventEnvelope>> {
        let stale_receiver = self.receiver.take()?;
        let (sender, receiver) = mpsc::unbounded_channel();
        self.sender = sender;
        if stale_receiver.len() >= STALE_LIVE_QUEUE_ASYNC_DROP_THRESHOLD
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let _ = runtime.spawn_blocking(move || drop(stale_receiver));
        } else {
            drop(stale_receiver);
        }
        Some(receiver)
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
            replay_view_revision: 0,
        }
    }
}
