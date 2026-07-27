use super::*;

#[derive(Debug)]
pub(crate) struct PatchHistoryCell {
    pub(super) id: PatchCellId,
    pub(super) changes: HashMap<PathBuf, FileChange>,
    pub(super) cwd: PathBuf,
}

impl HistoryCell for PatchHistoryCell {
    fn timeline_entry(&self) -> Option<PraxisTimelineAnchor> {
        Some(PraxisTimelineAnchor::new(
            praxis_app_core::PraxisTimelineKind::FileChange,
            praxis_app_core::PraxisTimelineTone::Success,
            format!(
                "{} file{} changed",
                self.changes.len(),
                if self.changes.len() == 1 { "" } else { "s" }
            ),
        ))
    }

    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let lines = create_patch_history_summary(&self.changes, &self.cwd, width as usize);
        let expanded = crate::history_presentation::is_diff_cell_expanded(self.id);
        if expanded || lines.len() <= 1 {
            return lines;
        }

        let hidden = lines.len().saturating_sub(1);
        vec![
            lines[0].clone(),
            Line::from(vec![
                "  └ ".dim(),
                format!("{hidden} preview lines hidden").dim(),
                " · ".dim(),
                DIFF_TOGGLE_KEY_HINT.blue().bold(),
                " expand".dim(),
            ]),
        ]
    }

    fn patch_cell_id(&self) -> Option<PatchCellId> {
        Some(self.id)
    }
}

#[derive(Debug)]
pub(crate) struct ResumePatchHistoryCell {
    pub(super) changes: HashMap<PathBuf, FileChange>,
    pub(super) cwd: PathBuf,
}

impl HistoryCell for ResumePatchHistoryCell {
    fn timeline_entry(&self) -> Option<PraxisTimelineAnchor> {
        Some(PraxisTimelineAnchor::new(
            praxis_app_core::PraxisTimelineKind::FileChange,
            praxis_app_core::PraxisTimelineTone::Neutral,
            format!(
                "{} resumed file change{}",
                self.changes.len(),
                if self.changes.len() == 1 { "" } else { "s" }
            ),
        ))
    }

    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut lines = create_diff_file_summary(&self.changes, &self.cwd);
        lines.push(Line::from(vec![
            "  └ ".dim(),
            "diff omitted from resume history".dim(),
        ]));
        lines
    }
}
