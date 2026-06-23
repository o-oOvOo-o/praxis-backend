use super::status_surfaces::TerminalTitleStatusKind;
use super::status_text::hook_event_label;
use super::*;
use crate::history_cell::PlainHistoryCell;
use crate::status_indicator_widget::StatusDetailsCapitalization;
use praxis_protocol::protocol::DeprecationNoticeEvent;
use praxis_protocol::protocol::HookCompletedEvent;
use praxis_protocol::protocol::HookStartedEvent;
#[cfg(test)]
use praxis_protocol::protocol::UndoCompletedEvent;
#[cfg(test)]
use praxis_protocol::protocol::UndoStartedEvent;
use tracing::debug;

impl ChatWidget {
    pub(super) fn on_shutdown_complete(&mut self) {
        self.request_immediate_exit();
    }

    pub(super) fn on_turn_diff(&mut self, unified_diff: String) {
        debug!("TurnDiffEvent: {unified_diff}");
        self.refresh_status_line();
    }

    pub(super) fn on_deprecation_notice(&mut self, event: DeprecationNoticeEvent) {
        let DeprecationNoticeEvent { summary, details } = event;
        self.add_to_history(history_cell::new_deprecation_notice(summary, details));
        self.request_redraw();
    }

    #[cfg(test)]
    pub(super) fn on_background_event(&mut self, message: String) {
        debug!("BackgroundEvent: {message}");
        self.bottom_pane.ensure_status_indicator();
        self.bottom_pane
            .set_interrupt_hint_visible(/*visible*/ true);
        self.terminal_title_status_kind = TerminalTitleStatusKind::Reasoning;
        self.set_status_header(message);
    }

    pub(super) fn on_hook_started(&mut self, event: HookStartedEvent) {
        let label = hook_event_label(event.run.event_name);
        let mut message = format!("Running {label} hook");
        if let Some(status_message) = event.run.status_message
            && !status_message.is_empty()
        {
            message.push_str(": ");
            message.push_str(&status_message);
        }
        self.add_to_history(history_cell::new_info_event(message, /*hint*/ None));
        self.request_redraw();
    }

    pub(super) fn on_hook_completed(&mut self, event: HookCompletedEvent) {
        let status = format!("{:?}", event.run.status).to_lowercase();
        let header = format!("{} hook ({status})", hook_event_label(event.run.event_name));
        let mut lines: Vec<ratatui::text::Line<'static>> = vec![header.into()];
        for entry in event.run.entries {
            let prefix = match entry.kind {
                praxis_protocol::protocol::HookOutputEntryKind::Warning => "warning: ",
                praxis_protocol::protocol::HookOutputEntryKind::Stop => "stop: ",
                praxis_protocol::protocol::HookOutputEntryKind::Feedback => "feedback: ",
                praxis_protocol::protocol::HookOutputEntryKind::Context => "hook context: ",
                praxis_protocol::protocol::HookOutputEntryKind::Error => "error: ",
            };
            lines.push(format!("  {prefix}{}", entry.text).into());
        }
        self.add_to_history(PlainHistoryCell::new(lines));
        self.request_redraw();
    }

    #[cfg(test)]
    pub(super) fn on_undo_started(&mut self, event: UndoStartedEvent) {
        self.bottom_pane.ensure_status_indicator();
        self.bottom_pane
            .set_interrupt_hint_visible(/*visible*/ false);
        let message = event
            .message
            .unwrap_or_else(|| "Undo in progress...".to_string());
        self.terminal_title_status_kind = TerminalTitleStatusKind::Undoing;
        self.set_status_header(message);
    }

    #[cfg(test)]
    pub(super) fn on_undo_completed(&mut self, event: UndoCompletedEvent) {
        let UndoCompletedEvent { success, message } = event;
        self.bottom_pane.hide_status_indicator();
        self.terminal_title_status_kind = TerminalTitleStatusKind::TurnRunning;
        self.refresh_terminal_title();
        let message = message.unwrap_or_else(|| {
            if success {
                "Undo completed successfully.".to_string()
            } else {
                "Undo failed.".to_string()
            }
        });
        if success {
            self.add_info_message(message, /*hint*/ None);
        } else {
            self.add_error_message(message);
        }
    }

    pub(super) fn on_stream_error(&mut self, message: String, additional_details: Option<String>) {
        if self.retry_status_header.is_none() {
            self.retry_status_header = Some(self.current_status.header.clone());
        }
        self.bottom_pane.ensure_status_indicator();
        self.terminal_title_status_kind = TerminalTitleStatusKind::Reasoning;
        self.set_status(
            message,
            additional_details,
            StatusDetailsCapitalization::CapitalizeFirst,
            STATUS_DETAILS_DEFAULT_MAX_LINES,
        );
    }
}
