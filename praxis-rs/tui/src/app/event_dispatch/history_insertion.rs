use super::*;

impl App {
    pub(super) fn insert_history_cell_now(
        &mut self,
        tui: &mut tui::Tui,
        cell: Box<dyn HistoryCell>,
    ) {
        self.active_thread_view_materialized_revision = None;
        let cell: Arc<dyn HistoryCell> = cell.into();
        if let Some(Overlay::Transcript(t)) = &mut self.overlay {
            t.insert_cell(cell.clone());
            if self.thread_replay_buffer_generation.is_none() {
                tui.frame_requester().schedule_frame();
            }
        }
        self.transcript_cells.push(cell);
        if self.thread_replay_buffer_generation.is_none() {
            tui.frame_requester().schedule_frame();
        }
    }

    pub(super) fn insert_history_cells_now(
        &mut self,
        tui: &mut tui::Tui,
        cells: Vec<Box<dyn HistoryCell>>,
    ) {
        if cells.is_empty() {
            return;
        }
        self.active_thread_view_materialized_revision = None;
        self.transcript_cells.reserve(cells.len());
        for cell in cells {
            let cell: Arc<dyn HistoryCell> = cell.into();
            if let Some(Overlay::Transcript(transcript)) = &mut self.overlay {
                transcript.insert_cell(cell.clone());
            }
            self.transcript_cells.push(cell);
        }
        if self.thread_replay_buffer_generation.is_none() {
            tui.frame_requester().schedule_frame();
        }
    }
}
