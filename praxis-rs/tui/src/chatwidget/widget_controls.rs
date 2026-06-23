use super::*;

impl ChatWidget {
    /// Exit the UI immediately without waiting for shutdown.
    ///
    /// Prefer [`Self::request_quit_without_confirmation`] for user-initiated exits;
    /// this is mainly a fallback for shutdown completion or emergency exits.
    pub(super) fn request_immediate_exit(&self) {
        self.app_event_tx.send(AppEvent::Exit(ExitMode::Immediate));
    }

    /// Request a shutdown-first quit.
    ///
    /// This is used for explicit quit commands (`/quit`, `/exit`, `/logout`) and for
    /// the double-press Ctrl+C/Ctrl+D quit shortcut.
    pub(super) fn request_quit_without_confirmation(&self) {
        self.app_event_tx
            .send(AppEvent::Exit(ExitMode::ShutdownFirst));
    }

    pub(super) fn request_redraw(&mut self) {
        self.frame_requester.schedule_frame();
    }

    pub(super) fn bump_active_cell_revision(&mut self) {
        // Wrapping avoids overflow; wraparound would require 2^64 bumps and at
        // worst causes a one-time cache-key collision.
        self.active_cell_revision = self.active_cell_revision.wrapping_add(1);
        self.active_cell_render_cache.borrow_mut().take();
        self.workspace_active_tail_cache.borrow_mut().take();
    }

    pub(super) fn notify(&mut self, notification: Notification) {
        if !matches!(notification, Notification::AgentTurnComplete { .. }) {
            let priority = notification.priority();
            let severity = if priority > 0 {
                ToastSeverity::Notice
            } else {
                ToastSeverity::Info
            };
            let duration = if priority > 0 {
                IN_APP_TOAST_PRIORITY_DURATION
            } else {
                IN_APP_TOAST_DURATION
            };
            self.show_in_app_toast(ToastEntry::new(
                notification.type_name(),
                notification.display(),
                severity,
                priority,
                duration,
            ));
        }
        if notification.allowed_for(&self.tui_config.notifications) {
            if let Some(existing) = self.pending_notification.as_ref()
                && existing.priority() > notification.priority()
            {
                self.request_redraw();
                return;
            }
            self.pending_notification = Some(notification);
        }
        self.request_redraw();
    }

    fn show_in_app_toast(&mut self, next_toast: ToastEntry) {
        self.in_app_toasts.enqueue(next_toast);
        if let Some(next_wakeup) = self.in_app_toasts.next_wakeup_in(Instant::now()) {
            self.frame_requester.schedule_frame_in(next_wakeup);
        }
    }

    pub(crate) fn show_info_toast(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.show_in_app_toast(ToastEntry::new(
            format!("info:{message}"),
            message,
            ToastSeverity::Info,
            0,
            IN_APP_TOAST_DURATION,
        ));
        self.request_redraw();
    }

    pub(super) fn show_error_toast(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.show_in_app_toast(ToastEntry::new(
            format!("error:{message}"),
            message,
            ToastSeverity::Error,
            2,
            IN_APP_TOAST_PRIORITY_DURATION,
        ));
        self.request_redraw();
    }

    pub(super) fn expire_in_app_toast(&mut self) {
        self.in_app_toasts.expire(Instant::now());
    }

    pub(crate) fn maybe_post_pending_notification(&mut self, tui: &mut crate::tui::Tui) {
        if let Some(notif) = self.pending_notification.take() {
            tui.notify(notif.display());
        }
    }
}
