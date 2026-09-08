use super::*;

impl ChatWidget {
    pub(super) fn regenerate_thread_name(&mut self) {
        self.session_telemetry
            .counter("praxis.thread.namegen", /*inc*/ 1, &[]);
        let Some(thread_id) = self.thread_id else {
            self.add_error_message(
                "Thread is not ready yet; cannot regenerate its name.".to_string(),
            );
            self.bottom_pane.drain_pending_submission_state();
            return;
        };
        self.app_event_tx
            .send(AppEvent::RegenerateThreadName { thread_id });
        self.bottom_pane.drain_pending_submission_state();
    }
}
