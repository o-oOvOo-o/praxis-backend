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

const THREAD_VIEW_CACHE_CAPACITY: usize = 2;

pub(super) struct CachedThreadView {
    pub(super) chat_widget: ChatWidget,
    pub(super) transcript_cells: Vec<Arc<dyn HistoryCell>>,
    pub(super) revision: u64,
}

#[derive(Default)]
pub(super) struct ThreadViewCache {
    entries: HashMap<ThreadId, CachedThreadView>,
    lru: VecDeque<ThreadId>,
}

impl ThreadViewCache {
    pub(super) fn take(&mut self, thread_id: ThreadId) -> Option<CachedThreadView> {
        let cached = self.entries.remove(&thread_id)?;
        self.remove_from_lru(thread_id);
        Some(cached)
    }

    pub(super) fn insert(&mut self, thread_id: ThreadId, view: CachedThreadView) {
        self.entries.insert(thread_id, view);
        self.remove_from_lru(thread_id);
        self.lru.push_back(thread_id);
        while self.entries.len() > THREAD_VIEW_CACHE_CAPACITY {
            let Some(evicted) = self.lru.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
    }

    fn remove_from_lru(&mut self, thread_id: ThreadId) {
        if let Some(position) = self.lru.iter().position(|cached| *cached == thread_id) {
            self.lru.remove(position);
        }
    }
}

impl App {
    pub(super) fn cacheable_active_thread_view_revision(&self, thread_id: ThreadId) -> Option<u64> {
        if self.thread_replay_buffer_generation.is_some()
            || self.overlay.is_some()
            || !self.chat_widget.no_modal_or_popup_active()
            || self.chat_widget.realtime_conversation_is_live()
            || self.chat_widget.thread_id() != Some(thread_id)
        {
            return None;
        }
        let materialized = self.active_thread_view_materialized_revision?;
        let channel = self.thread_event_channels.get(&thread_id)?;
        let store = lock_thread_event_store(&channel.store);
        let status = store.status.as_ref()?;
        if status.is_active() {
            return None;
        }
        (store.snapshot_revision() == materialized).then_some(materialized)
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
        self.thread_view_cache.insert(
            thread_id,
            CachedThreadView {
                chat_widget,
                transcript_cells,
                revision,
            },
        );
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
                    .map(|channel| channel.replay_snapshot_revision)
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
        if store.last_event_sequence() != processed_sequence
            || store
                .status
                .as_ref()
                .is_none_or(|status| status.is_active())
        {
            return;
        }
        let revision = store.snapshot_revision();
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
        if store.snapshot_revision() == revision
            && store
                .status
                .as_ref()
                .is_some_and(|status| !status.is_active())
        {
            drop(store);
            self.active_thread_view_materialized_revision = Some(revision);
        }
    }
}
