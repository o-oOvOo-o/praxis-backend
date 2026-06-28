use crate::history_cell::HistoryCellMouseAction;
use crate::history_presentation::PatchCellId;
use ratatui::text::Line;

#[derive(Clone, Debug)]
pub(crate) struct TranscriptVisibleRow {
    pub(crate) line: Line<'static>,
    pub(crate) patch_cell_ids: Vec<PatchCellId>,
    pub(crate) mouse_actions: Vec<HistoryCellMouseAction>,
    pub(crate) selectable_range: Option<(u16, u16)>,
}

impl TranscriptVisibleRow {
    pub(crate) fn new(
        line: Line<'static>,
        patch_cell_ids: Vec<PatchCellId>,
        mouse_actions: Vec<HistoryCellMouseAction>,
    ) -> Self {
        let selectable_range = selectable_range_for_line(&line);
        Self {
            line,
            patch_cell_ids,
            mouse_actions,
            selectable_range,
        }
    }

    pub(crate) fn blank() -> Self {
        Self {
            line: Line::from(""),
            patch_cell_ids: Vec::new(),
            mouse_actions: Vec::new(),
            selectable_range: None,
        }
    }
}

fn selectable_range_for_line(line: &Line<'_>) -> Option<(u16, u16)> {
    let rendered = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let mut first = None;
    let mut last = None;
    for (index, ch) in rendered.chars().enumerate() {
        if !ch.is_whitespace() {
            first.get_or_insert(index);
            last = Some(index);
        }
    }
    let first = first?;
    let last = last?;
    Some((
        u16::try_from(first).unwrap_or(u16::MAX),
        u16::try_from(last).unwrap_or(u16::MAX),
    ))
}
