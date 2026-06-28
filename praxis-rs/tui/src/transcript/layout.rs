use crate::history_cell::ChatLane;
use crate::history_cell::HistoryCellMouseAction;
use crate::history_presentation::PatchCellId;

#[derive(Clone, Debug)]
pub(crate) struct TranscriptBlock {
    pub(crate) lane: ChatLane,
    pub(crate) lines: Vec<ratatui::text::Line<'static>>,
    pub(crate) patch_cell_ids: Vec<PatchCellId>,
    pub(crate) mouse_actions: Vec<HistoryCellMouseAction>,
    pub(crate) row_start: usize,
}

impl TranscriptBlock {
    pub(crate) fn new(
        lane: ChatLane,
        lines: Vec<ratatui::text::Line<'static>>,
        patch_cell_ids: Vec<PatchCellId>,
        mouse_actions: Vec<HistoryCellMouseAction>,
        row_start: usize,
    ) -> Self {
        Self {
            lane,
            lines,
            patch_cell_ids,
            mouse_actions,
            row_start,
        }
    }
}
