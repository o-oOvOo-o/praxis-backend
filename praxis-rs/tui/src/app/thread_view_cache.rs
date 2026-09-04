use crate::app_event::AppEvent;
use crate::chatwidget::ChatWidget;
use crate::history_cell::HistoryCell;
use crate::tui;
use color_eyre::eyre::Result;
use praxis_protocol::ThreadId;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use super::App;
use super::lock_thread_event_store;

const THREAD_VIEW_CACHE_MAX_ENTRIES: usize = 4;
const THREAD_VIEW_CACHE_MAX_TRANSCRIPT_CELLS: usize = 400;

pub(super) struct CachedThreadView {
    pub(super) chat_widget: ChatWidget,
    pub(super) transcript_cells: Vec<Arc<dyn HistoryCell>>,
    pub(super) revision: u64,
}

#[derive(Default)]
pub(super) struct ThreadViewCache {
    entries: HashMap<ThreadId, CachedThreadView>,
    lru: VecDeque<ThreadId>,
    transcript_cells: usize,
}

impl ThreadViewCache {
    pub(super) fn take(&mut self, thread_id: ThreadId) -> Option<CachedThreadView> {
        let cached = self.entries.remove(&thread_id)?;
        self.transcript_cells = self
            .transcript_cells
            .saturating_sub(cached.transcript_cells.len());
        self.remove_from_lru(thread_id);
        Some(cached)
    }

    pub(super) fn insert(
        &mut self,
        thread_id: ThreadId,
        view: CachedThreadView,
    ) -> Vec<(ThreadId, CachedThreadView)> {
        let mut evicted_views = Vec::new();
        let transcript_cells = view.transcript_cells.len();
        self.remove_from_lru(thread_id);
        if let Some(replaced) = self.entries.insert(thread_id, view) {
            self.transcript_cells = self
                .transcript_cells
                .saturating_sub(replaced.transcript_cells.len());
        }
        self.transcript_cells = self.transcript_cells.saturating_add(transcript_cells);
        self.lru.push_back(thread_id);
        while self.entries.len() > 1
            && (self.entries.len() > THREAD_VIEW_CACHE_MAX_ENTRIES
                || self.transcript_cells > THREAD_VIEW_CACHE_MAX_TRANSCRIPT_CELLS)
        {
            let Some(evicted) = self.lru.pop_front() else {
                break;
            };
            if let Some(view) = self.entries.remove(&evicted) {
                self.transcript_cells = self
                    .transcript_cells
                    .saturating_sub(view.transcript_cells.len());
                evicted_views.push((evicted, view));
            }
        }
        evicted_views
    }

    pub(super) fn drain(&mut self) -> Vec<(ThreadId, CachedThreadView)> {
        let entries = self.entries.drain().collect();
        self.lru.clear();
        self.transcript_cells = 0;
        entries
    }

    fn remove_from_lru(&mut self, thread_id: ThreadId) {
        if let Some(position) = self.lru.iter().position(|cached| *cached == thread_id) {
            self.lru.remove(position);
        }
    }
}

impl App {
    pub(super) fn suspend_active_thread_view_for_switch(
        &mut self,
    ) -> (Option<ThreadId>, Option<u64>) {
        let Some(thread_id) = self.active_thread_id else {
            return (None, None);
        };
        let view_is_cache_candidate = self.thread_replay_buffer_generation.is_none()
            && self.overlay.is_none()
            && self.chat_widget.no_modal_or_popup_active()
            && !self.chat_widget.realtime_conversation_is_live()
            && self.chat_widget.thread_id() == Some(thread_id)
            && self.active_thread_view_materialized_revision.is_some();
        let input_state_is_deferred = view_is_cache_candidate && self.workspace.enabled;
        let input_state =
            (!input_state_is_deferred).then(|| self.chat_widget.capture_thread_input_state());
        let materialized_revision = self.active_thread_view_materialized_revision;
        let Some(channel) = self.thread_event_channels.get_mut(&thread_id) else {
            return (Some(thread_id), None);
        };
        let receiver = self.active_thread_rx.take();
        let store_handle = Arc::clone(&channel.store);
        let revision = {
            let mut store = lock_thread_event_store(&store_handle);
            store.active = false;
            if let Some(input_state) = input_state {
                store.input_state = input_state;
            }
            view_is_cache_candidate
                .then(|| {
                    let materialized = materialized_revision?;
                    // Running views are safe to retain too: any transcript-changing event
                    // advances the store revision and rejects this cached revision on restore.
                    (store.view_revision() == materialized).then_some(materialized)
                })
                .flatten()
        };
        if let Some(receiver) = receiver {
            channel.receiver = Some(receiver);
        }
        if input_state_is_deferred && revision.is_none() {
            let input_state = self.chat_widget.capture_thread_input_state();
            lock_thread_event_store(&store_handle).input_state = input_state;
        }
        (Some(thread_id), revision)
    }

    pub(super) fn take_cached_thread_view_candidate(
        &mut self,
        thread_id: ThreadId,
    ) -> Option<CachedThreadView> {
        self.thread_view_cache.take(thread_id)
    }

    pub(super) fn cache_replaced_thread_view(
        &mut self,
        thread_id: ThreadId,
        revision: u64,
        chat_widget: ChatWidget,
        transcript_cells: Vec<Arc<dyn HistoryCell>>,
    ) {
        self.cache_thread_view(
            thread_id,
            CachedThreadView {
                chat_widget,
                transcript_cells,
                revision,
            },
        );
    }

    pub(super) fn cache_thread_view(&mut self, thread_id: ThreadId, view: CachedThreadView) {
        let evicted_views = self.thread_view_cache.insert(thread_id, view);
        for (evicted_thread_id, evicted_view) in evicted_views {
            self.persist_cached_thread_input_state(evicted_thread_id, &evicted_view);
        }
    }

    pub(super) fn persist_cached_thread_input_state(
        &mut self,
        thread_id: ThreadId,
        cached_view: &CachedThreadView,
    ) {
        let Some(channel) = self.thread_event_channels.get(&thread_id) else {
            return;
        };
        let input_state = cached_view.chat_widget.capture_thread_input_state();
        lock_thread_event_store(&channel.store).input_state = input_state;
    }

    pub(super) fn carry_cached_thread_input_state_into_snapshot(
        &mut self,
        thread_id: ThreadId,
        cached_view: &CachedThreadView,
        snapshot: &mut super::thread_event_store::ThreadEventSnapshot,
    ) {
        let input_state = cached_view.chat_widget.capture_thread_input_state();
        snapshot.input_state = input_state.clone();
        if let Some(channel) = self.thread_event_channels.get(&thread_id) {
            lock_thread_event_store(&channel.store).input_state = input_state;
        }
    }

    pub(super) fn clear_thread_view_cache_preserving_input(&mut self) {
        for (thread_id, cached_view) in self.thread_view_cache.drain() {
            self.persist_cached_thread_input_state(thread_id, &cached_view);
        }
    }

    pub(super) fn install_thread_view_shell(
        &mut self,
        tui: &mut tui::Tui,
        target_widget: ChatWidget,
        cached_transcript: Option<Vec<Arc<dyn HistoryCell>>>,
        previous_view: Option<(ThreadId, u64)>,
        target_revision: u64,
    ) -> Result<bool> {
        let reused_cached_view = cached_transcript.is_some();
        let previous_transcript = std::mem::take(&mut self.transcript_cells);
        let previous_widget = self.replace_chat_widget(target_widget);
        self.reset_for_thread_switch(tui)?;
        if let Some((previous_thread_id, previous_revision)) = previous_view {
            self.cache_replaced_thread_view(
                previous_thread_id,
                previous_revision,
                previous_widget,
                previous_transcript,
            );
        }
        if let Some(cached_transcript) = cached_transcript {
            self.transcript_cells = cached_transcript;
            self.active_thread_view_materialized_revision = Some(target_revision);
            tui.frame_requester().schedule_frame();
        } else {
            self.active_thread_view_materialized_revision = None;
        }
        Ok(reused_cached_view)
    }

    pub(super) fn mark_active_thread_view_materialized(&mut self) {
        self.active_thread_view_materialized_revision =
            self.active_thread_id.and_then(|thread_id| {
                self.thread_event_channels
                    .get(&thread_id)
                    .map(|channel| channel.replay_view_revision)
            });
    }

    pub(super) fn queue_thread_view_materialized_marker(
        &self,
        thread_id: ThreadId,
        processed_sequence: u64,
    ) {
        if self.active_thread_id != Some(thread_id) {
            return;
        }
        let Some(channel) = self.thread_event_channels.get(&thread_id) else {
            return;
        };
        let store = lock_thread_event_store(&channel.store);
        if store.last_event_sequence() != processed_sequence {
            return;
        }
        let revision = store.view_revision();
        drop(store);
        self.app_event_tx
            .send(AppEvent::MarkThreadViewMaterialized {
                thread_id,
                generation: self.history_view_generation,
                revision,
            });
    }

    pub(super) fn apply_thread_view_materialized_marker(
        &mut self,
        thread_id: ThreadId,
        generation: u64,
        revision: u64,
    ) {
        if self.active_thread_id != Some(thread_id) || self.history_view_generation != generation {
            return;
        }
        let Some(channel) = self.thread_event_channels.get(&thread_id) else {
            return;
        };
        let store = lock_thread_event_store(&channel.store);
        if store.view_revision() == revision {
            drop(store);
            self.active_thread_view_materialized_revision = Some(revision);
        }
    }
}
