use super::App;
use super::thread_event_channels::CachedThreadProjectionState;
use super::thread_view_cache::CachedThreadView;
use crate::chatwidget::ChatWidget;
use crate::tui;
use color_eyre::eyre::Result;
use praxis_protocol::ThreadId;

impl App {
    pub(super) fn restore_thread_view(
        &mut self,
        tui: &mut tui::Tui,
        thread_id: ThreadId,
        target_revision: u64,
        previous_view: Option<(ThreadId, u64)>,
        cached_view: Option<CachedThreadView>,
        cached_projection: Option<CachedThreadProjectionState>,
    ) -> Result<bool> {
        if let Some(timing) = self.thread_view_cache.switch_timing.as_mut() {
            timing.cached = Some(cached_view.is_some());
        }
        self.advance_history_view_generation();
        let (mut target_widget, cached_transcript) = if let Some(cached) = cached_view {
            tracing::debug!(
                thread_id = %thread_id,
                revision = target_revision,
                "restoring materialized thread view from cache"
            );
            cached
                .chat_widget
                .rebind_history_view(self.history_view_generation);
            (cached.chat_widget, Some(cached.transcript_cells))
        } else {
            let init = self.chatwidget_init_for_forked_or_resumed_thread(
                tui,
                self.config.clone(),
                self.tui_config.clone(),
            );
            (ChatWidget::new_for_thread_replay(init), None)
        };
        // Batch cached projections into one presentation commit.
        let projection_refresh_batch = cached_projection
            .as_ref()
            .is_some_and(|projection| !projection.is_empty());
        if projection_refresh_batch {
            target_widget.begin_status_surface_refresh_batch();
            target_widget.begin_redraw_batch();
        }
        let cached_events = cached_projection
            .map(|projection| projection.apply_to(&mut target_widget))
            .unwrap_or_default();
        let result = self.install_thread_view_shell(
            tui,
            target_widget,
            cached_transcript,
            previous_view,
            target_revision,
        );
        if projection_refresh_batch {
            self.chat_widget.finish_status_surface_refresh_batch();
            self.refresh_status_line();
            self.chat_widget.finish_redraw_batch();
        }
        let reused_cached_view = result?;
        if reused_cached_view {
            self.queue_thread_view_catchup(thread_id, target_revision, cached_events);
        }
        Ok(reused_cached_view)
    }
}
