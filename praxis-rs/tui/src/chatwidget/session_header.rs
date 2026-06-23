use super::ChatWidget;
use crate::history_cell;
use crate::history_cell::HistoryCell;

pub(crate) struct SessionHeader {
    model: String,
}

impl SessionHeader {
    pub(crate) fn new(model: String) -> Self {
        Self { model }
    }

    /// Updates the header's model text.
    pub(crate) fn set_model(&mut self, model: &str) {
        if self.model != model {
            self.model = model.to_string();
        }
    }
}

impl ChatWidget {
    /// Merge the real session info cell with any placeholder header to avoid double boxes.
    pub(super) fn apply_session_info_cell(&mut self, cell: history_cell::SessionInfoCell) {
        if cell.display_lines(u16::MAX).is_empty() {
            return;
        }
        let mut session_info_cell = Some(Box::new(cell) as Box<dyn HistoryCell>);
        let merged_header = if let Some(active) = self.active_cell.take() {
            if active
                .as_any()
                .is::<history_cell::SessionHeaderHistoryCell>()
            {
                if let Some(cell) = session_info_cell.take() {
                    self.active_cell = Some(cell);
                }
                true
            } else {
                self.active_cell = Some(active);
                false
            }
        } else {
            false
        };

        if merged_header {
            self.bump_active_cell_revision();
            self.request_redraw();
            return;
        }

        self.flush_active_cell();

        if !merged_header && let Some(cell) = session_info_cell {
            self.add_boxed_history(cell);
        }
    }
}
