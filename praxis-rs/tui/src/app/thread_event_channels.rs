use super::App;
use super::thread_event_store::CachedViewDelta;
use super::thread_event_store::ThreadBufferedEvent;
use super::thread_event_store::ThreadEventChannel;
use super::thread_event_store::ThreadEventEnvelope;
use super::thread_event_store::ThreadEventStore;
use super::thread_event_store::lock_thread_event_store;
use crate::app_command::AppCommand;
use crate::app_gateway_session::AppGatewaySession;
use crate::app_gateway_session::ThreadSessionState;
use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::chatwidget::ChatWidget;
use crate::chatwidget::ReplayKind;
use crate::chatwidget::ThreadInputState;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_app_gateway_protocol::Turn;
use praxis_protocol::ThreadId;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc;

// Live delivery preserves FIFO while this cap bounds the durable replay projection.
const THREAD_EVENT_REPLAY_CAPACITY: usize = 32768;

#[derive(Debug)]
pub(super) struct ThreadReplaySource {
    pub(super) session: Option<ThreadSessionState>,
    pub(super) turns: Arc<Vec<Turn>>,
    pub(super) status: Option<ThreadStatus>,
    pub(super) control_state: Option<ThreadControlState>,
    pub(super) token_usage: Option<ThreadTokenUsage>,
    pub(super) events: Vec<ThreadBufferedEvent>,
    pub(super) input_state: Option<ThreadInputState>,
}

impl ThreadReplaySource {
    fn new(store: &ThreadEventStore) -> Self {
        Self {
            session: store.session.clone(),
            turns: Arc::clone(&store.turns),
            status: store.status.clone(),
            control_state: store.control_state.clone(),
            token_usage: store.latest_token_usage(),
            events: store.snapshot_event_refs().into_iter().cloned().collect(),
            input_state: store.input_state.clone(),
        }
    }

    pub(super) fn has_session(&self) -> bool {
        self.session.is_some()
    }
}

pub(super) struct CachedThreadProjectionState {
    surface: Option<CachedThreadSurfaceState>,
    pub(super) events: VecDeque<ThreadEventEnvelope>,
}

struct CachedThreadSurfaceState {
    status: Option<praxis_app_gateway_protocol::ThreadStatus>,
    control_state: Option<praxis_app_gateway_protocol::ThreadControlState>,
    token_usage: Option<praxis_app_gateway_protocol::ThreadTokenUsage>,
    goal_notification: Option<praxis_app_gateway_protocol::ServerNotification>,
}

impl CachedThreadProjectionState {
    pub(super) fn is_empty(&self) -> bool {
        self.surface.is_none() && self.events.is_empty()
    }

    pub(super) fn apply_to(self, chat_widget: &mut ChatWidget) -> VecDeque<ThreadEventEnvelope> {
        if let Some(surface) = self.surface {
            chat_widget.apply_thread_status_snapshot(surface.status.as_ref());
            chat_widget.set_thread_control_state(surface.control_state.as_ref());
            if let Some(token_usage) = surface.token_usage {
                chat_widget.set_token_info(Some(token_usage_info_from_app_gateway(token_usage)));
            }
            if let Some(notification) = surface.goal_notification {
                chat_widget
                    .handle_server_notification(notification, Some(ReplayKind::ThreadSnapshot));
            }
        }
        self.events
    }
}

pub(super) enum ThreadViewActivationState {
    Cached(CachedThreadProjectionState),
    Replay(ThreadReplaySource),
}

impl App {
    pub(super) async fn shutdown_current_thread(&mut self, app_gateway: &mut AppGatewaySession) {
        if let Some(thread_id) = self.chat_widget.thread_id() {
            // Clear any in-flight rollback guard when switching threads.
            self.backtrack.pending_rollback = None;
            if let Err(err) = app_gateway.thread_unsubscribe(thread_id).await {
                tracing::warn!("failed to unsubscribe thread {thread_id}: {err}");
            }
            self.abort_thread_event_listener(thread_id);
        }
    }

    fn abort_thread_event_listener(&mut self, thread_id: ThreadId) {
        if let Some(handle) = self.thread_event_listener_tasks.remove(&thread_id) {
            handle.abort();
        }
    }

    pub(super) fn abort_all_thread_event_listeners(&mut self) {
        for handle in self
            .thread_event_listener_tasks
            .drain()
            .map(|(_, handle)| handle)
        {
            handle.abort();
        }
    }

    pub(super) fn ensure_thread_channel(&mut self, thread_id: ThreadId) -> &mut ThreadEventChannel {
        self.thread_event_channels
            .entry(thread_id)
            .or_insert_with(|| ThreadEventChannel::new(THREAD_EVENT_REPLAY_CAPACITY))
    }

    fn set_thread_active(&mut self, thread_id: ThreadId, active: bool) {
        if let Some(channel) = self.thread_event_channels.get_mut(&thread_id) {
            let mut store = lock_thread_event_store(&channel.store);
            store.active = active;
        }
    }

    pub(super) fn activate_thread_channel(&mut self, thread_id: ThreadId) {
        if self.active_thread_id.is_some() {
            return;
        }
        self.set_thread_active(thread_id, /*active*/ true);
        let receiver = if let Some(channel) = self.thread_event_channels.get_mut(&thread_id) {
            channel.receiver.take()
        } else {
            None
        };
        self.active_thread_id = Some(thread_id);
        self.active_thread_rx = receiver;
        self.refresh_pending_thread_approvals();
    }

    pub(super) fn store_active_thread_receiver(
        &mut self,
        input_state_retained_by_view_cache: bool,
    ) {
        let Some(active_id) = self.active_thread_id else {
            return;
        };
        let input_state = (!input_state_retained_by_view_cache)
            .then(|| self.chat_widget.capture_thread_input_state());
        if let Some(channel) = self.thread_event_channels.get_mut(&active_id) {
            let receiver = self.active_thread_rx.take();
            let mut store = lock_thread_event_store(&channel.store);
            store.active = false;
            if let Some(input_state) = input_state {
                store.input_state = input_state;
            }
            if let Some(receiver) = receiver {
                channel.receiver = Some(receiver);
            }
        }
    }

    pub(super) fn activate_thread_for_replay(
        &mut self,
        thread_id: ThreadId,
    ) -> Option<(
        mpsc::UnboundedReceiver<ThreadEventEnvelope>,
        ThreadReplaySource,
    )> {
        let channel = self.thread_event_channels.get_mut(&thread_id)?;
        let receiver = channel.take_rebased_receiver()?;
        let mut store = lock_thread_event_store(&channel.store);
        store.active = true;
        channel.replay_through_sequence = store.last_event_sequence();
        channel.replay_view_revision = store.view_revision();
        let replay_source = ThreadReplaySource::new(&store);
        store.begin_cached_view_delta(channel.replay_view_revision);
        Some((receiver, replay_source))
    }

    pub(super) fn activate_thread_for_view(
        &mut self,
        thread_id: ThreadId,
        cached_revision: Option<u64>,
    ) -> Option<(
        mpsc::UnboundedReceiver<ThreadEventEnvelope>,
        ThreadViewActivationState,
        u64,
    )> {
        let channel = self.thread_event_channels.get_mut(&thread_id)?;
        let receiver = channel.take_rebased_receiver()?;
        let mut store = lock_thread_event_store(&channel.store);
        store.active = true;
        channel.replay_through_sequence = store.last_event_sequence();
        let revision = store.view_revision();
        channel.replay_view_revision = revision;
        let cached_delta = cached_revision
            .and_then(|cached_revision| store.take_cached_view_delta(cached_revision));
        let state = if store.session.is_some()
            && let Some(CachedViewDelta {
                events,
                surface_dirty,
            }) = cached_delta
        {
            ThreadViewActivationState::Cached(CachedThreadProjectionState {
                surface: surface_dirty.then(|| CachedThreadSurfaceState {
                    status: store.status.clone(),
                    control_state: store.control_state.clone(),
                    token_usage: store.latest_token_usage(),
                    goal_notification: store.latest_goal_notification(),
                }),
                events,
            })
        } else {
            store.clear_cached_view_delta();
            ThreadViewActivationState::Replay(ThreadReplaySource::new(&store))
        };
        store.begin_cached_view_delta(revision);
        Some((receiver, state, revision))
    }

    pub(super) fn thread_session_state(&self, thread_id: ThreadId) -> Option<ThreadSessionState> {
        let channel = self.thread_event_channels.get(&thread_id)?;
        lock_thread_event_store(&channel.store).session.clone()
    }

    pub(super) fn active_thread_event_is_after_replay(
        &self,
        envelope: &ThreadEventEnvelope,
    ) -> bool {
        let Some(thread_id) = self.active_thread_id else {
            return false;
        };
        self.thread_event_channels
            .get(&thread_id)
            .is_some_and(|channel| envelope.sequence > channel.replay_through_sequence)
    }

    pub(super) fn clear_active_thread(&mut self) {
        if let Some(active_id) = self.active_thread_id.take() {
            self.set_thread_active(active_id, /*active*/ false);
        }
        self.active_thread_rx = None;
        self.active_thread_view_materialized_revision = None;
        self.clear_thread_view_catchup();
        self.refresh_pending_thread_approvals();
    }

    pub(super) fn note_thread_outbound_op(&mut self, thread_id: ThreadId, op: &AppCommand) {
        let has_pending = {
            let Some(channel) = self.thread_event_channels.get(&thread_id) else {
                return;
            };
            let mut store = lock_thread_event_store(&channel.store);
            store.note_outbound_op(op);
            store.has_pending_thread_approvals()
        };
        self.set_pending_thread_approval_state(thread_id, has_pending);
    }

    pub(super) fn note_active_thread_outbound_op(&mut self, op: &AppCommand) {
        if !ThreadEventStore::op_can_change_pending_replay_state(op) {
            return;
        }
        let Some(thread_id) = self.active_thread_id else {
            return;
        };
        self.note_thread_outbound_op(thread_id, op);
    }

    pub(super) fn active_turn_id_for_thread(&self, thread_id: ThreadId) -> Option<String> {
        let channel = self.thread_event_channels.get(&thread_id)?;
        let store = lock_thread_event_store(&channel.store);
        store.active_turn_id().map(ToOwned::to_owned)
    }
}
