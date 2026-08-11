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
    /// Commit visible session diagnostics without creating a persistent welcome cell.
    pub(super) fn apply_session_info_cell(&mut self, cell: history_cell::SessionInfoCell) {
        if cell.display_lines(u16::MAX).is_empty() {
            return;
        }
        self.flush_active_cell();
        self.add_boxed_history(Box::new(cell) as Box<dyn HistoryCell>);
    }
}
