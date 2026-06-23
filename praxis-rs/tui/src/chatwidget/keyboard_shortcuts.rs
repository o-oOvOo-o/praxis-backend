use super::*;

impl ChatWidget {
    /// Handles a Ctrl+C press at the chat-widget layer.
    ///
    /// Running work treats Ctrl+C as interrupt-only. Idle Ctrl+C requires a second press before
    /// shutdown, so accidental terminal control keys cannot close a live session.
    ///
    /// Active realtime conversations take precedence over bottom-pane Ctrl+C handling so the
    /// first press always stops live voice, even when the composer contains the recording meter.
    ///
    /// If the same quit shortcut is pressed again before expiry, this requests a shutdown-first
    /// quit.
    pub(super) fn on_ctrl_c(&mut self) {
        let key = key_hint::ctrl(KeyCode::Char('c'));
        if self.realtime_conversation.is_live() {
            self.clear_quit_shortcut();
            self.stop_realtime_conversation_from_ui();
            return;
        }
        if self.bottom_pane.on_ctrl_c() == CancellationEvent::Handled {
            self.clear_quit_shortcut();
            return;
        }

        if self.is_cancellable_work_active() {
            self.clear_quit_shortcut();
            self.submit_op(AppCommand::interrupt());
            return;
        }

        if self.quit_shortcut_active_for(key) {
            self.quit_shortcut_expires_at = None;
            self.quit_shortcut_key = None;
            self.request_quit_without_confirmation();
            return;
        }

        self.arm_quit_shortcut(key);
    }

    /// Handles a Ctrl+D press at the chat-widget layer.
    ///
    /// Ctrl-D only participates in quit when the composer is empty and no modal/popup is active.
    /// Otherwise it should be routed to the active view and not attempt to quit.
    pub(super) fn on_ctrl_d(&mut self) -> bool {
        let key = key_hint::ctrl(KeyCode::Char('d'));
        if !self.bottom_pane.composer_is_empty() || !self.bottom_pane.no_modal_or_popup_active() {
            return false;
        }

        if self.is_cancellable_work_active() {
            self.clear_quit_shortcut();
            return true;
        }

        if self.quit_shortcut_active_for(key) {
            self.quit_shortcut_expires_at = None;
            self.quit_shortcut_key = None;
            self.request_quit_without_confirmation();
            return true;
        }

        self.arm_quit_shortcut(key);
        true
    }

    /// True if `key` matches the armed quit shortcut and the window has not expired.
    fn quit_shortcut_active_for(&self, key: KeyBinding) -> bool {
        self.quit_shortcut_key == Some(key)
            && self
                .quit_shortcut_expires_at
                .is_some_and(|expires_at| Instant::now() < expires_at)
    }

    /// Arm the double-press quit shortcut and show the footer hint.
    ///
    /// This keeps the state machine (`quit_shortcut_*`) in `ChatWidget`, since
    /// it is the component that interprets Ctrl+C vs Ctrl+D and decides whether
    /// quitting is currently allowed, while delegating rendering to `BottomPane`.
    fn arm_quit_shortcut(&mut self, key: KeyBinding) {
        self.quit_shortcut_expires_at = Instant::now()
            .checked_add(QUIT_SHORTCUT_TIMEOUT)
            .or_else(|| Some(Instant::now()));
        self.quit_shortcut_key = Some(key);
        self.bottom_pane.show_quit_shortcut_hint(key);
    }

    fn clear_quit_shortcut(&mut self) {
        self.quit_shortcut_expires_at = None;
        self.quit_shortcut_key = None;
        self.bottom_pane.clear_quit_shortcut_hint();
    }

    // Review mode counts as cancellable work so Ctrl+C interrupts instead of quitting.
    fn is_cancellable_work_active(&self) -> bool {
        self.bottom_pane.is_task_running() || self.is_review_mode
    }

    pub(super) fn is_plan_streaming_in_tui(&self) -> bool {
        self.plan_stream_controller.is_some()
    }
}
