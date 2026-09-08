use super::App;
use super::thread_switch_timing::ThreadSwitchTiming;
use praxis_protocol::ThreadId;
use std::time::{Duration, Instant};

impl App {
    pub(super) fn begin_thread_switch_measurement(&mut self, target: ThreadId) {
        self.thread_view_cache.switch_timing =
            Some(ThreadSwitchTiming::new(target, Instant::now()));
    }

    pub(super) fn finish_thread_switch_frame(&mut self, draw_elapsed: Duration) {
        let Some(active) = self.active_thread_id else {
            return;
        };
        let ready = self.thread_replay_buffer_generation.is_none()
            && self.active_thread_view_materialized_revision.is_some()
            && !self.has_pending_thread_view_catchup();
        let Some(timing) = self.thread_view_cache.switch_timing.as_mut() else {
            return;
        };
        let Some(frame) = timing.frame(&active, ready, Instant::now()) else {
            return;
        };
        tracing::info!(target: "praxis_tui::thread_switch", thread_id = %active,
            cached = ?timing.cached, first_frame = frame.first_frame, ready = frame.complete,
            elapsed_us = frame.elapsed.as_micros() as u64,
            draw_us = draw_elapsed.as_micros() as u64,
            "thread switch frame committed");
        if frame.complete {
            self.thread_view_cache.switch_timing = None;
            #[cfg(debug_assertions)]
            self.advance_thread_switch_probe();
        }
    }
}
