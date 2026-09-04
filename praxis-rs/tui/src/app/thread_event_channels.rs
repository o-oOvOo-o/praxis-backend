use super::App;
use super::thread_event_store::ThreadEventChannel;
use super::thread_event_store::ThreadEventEnvelope;
use super::thread_event_store::ThreadEventSnapshot;
use super::thread_event_store::ThreadEventStore;
use super::thread_event_store::lock_thread_event_store;
use crate::app_command::AppCommand;
use crate::app_gateway_session::AppGatewaySession;
use crate::app_gateway_session::ThreadSessionState;
use praxis_protocol::ThreadId;
use tokio::sync::mpsc;

// Live delivery preserves FIFO while this cap bounds the durable replay projection.
const THREAD_EVENT_REPLAY_CAPACITY: usize = 32768;

pub(super) enum ThreadViewActivationState {
    Cached(ThreadSessionState),
    Replay(ThreadEventSnapshot),
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

    pub(super) fn store_active_thread_receiver(&mut self) {
        let Some(active_id) = self.active_thread_id else {
            return;
        };
        let input_state = self.chat_widget.capture_thread_input_state();
        if let Some(channel) = self.thread_event_channels.get_mut(&active_id) {
            let receiver = self.active_thread_rx.take();
            let mut store = lock_thread_event_store(&channel.store);
            store.active = false;
            store.input_state = input_state;
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
        ThreadEventSnapshot,
    )> {
        let channel = self.thread_event_channels.get_mut(&thread_id)?;
        let receiver = channel.receiver.take()?;
        let mut store = lock_thread_event_store(&channel.store);
        store.active = true;
        channel.replay_through_sequence = store.last_event_sequence();
        channel.replay_snapshot_revision = store.snapshot_revision();
        let snapshot = store.snapshot();
        Some((receiver, snapshot))
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
        let receiver = channel.receiver.take()?;
        let mut store = lock_thread_event_store(&channel.store);
        store.active = true;
        channel.replay_through_sequence = store.last_event_sequence();
        let revision = store.snapshot_revision();
        channel.replay_snapshot_revision = revision;
        let state = match (cached_revision == Some(revision), store.session.clone()) {
            (true, Some(session)) => ThreadViewActivationState::Cached(session),
            _ => ThreadViewActivationState::Replay(store.snapshot()),
        };
        Some((receiver, state, revision))
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
