use super::*;

#[derive(Debug)]
pub(crate) struct SessionInfoCell(CompositeHistoryCell);

impl HistoryCell for SessionInfoCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.display_lines(width)
    }

    fn committed_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.committed_display_lines(width)
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.0.desired_height(width)
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.0.transcript_lines(width)
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        self.0.transcript_animation_tick()
    }

    fn mouse_targets(&self, width: u16) -> Vec<HistoryCellMouseTarget> {
        self.0.mouse_targets(width)
    }
}

/// Returns the only session-configuration transcript output that remains useful:
/// a warning when the backend selected a different model than the caller requested.
///
/// The former welcome/header cell deliberately does not live in history. Workspace
/// owns the single empty-thread entry surface, so new and resumed sessions cannot
/// diverge into competing welcome experiences.
pub(crate) fn new_session_info(
    requested_model: &str,
    event: SessionConfiguredEvent,
    report_model_substitution: bool,
) -> SessionInfoCell {
    let mut parts: Vec<Box<dyn HistoryCell>> = Vec::new();
    if report_model_substitution && requested_model != event.model {
        let lines = vec![
            "model changed:".magenta().bold().into(),
            format!("requested: {requested_model}").into(),
            format!("used: {}", event.model).into(),
        ];
        parts.push(Box::new(PlainHistoryCell { lines }));
    }

    SessionInfoCell(CompositeHistoryCell { parts })
}

pub(crate) fn new_user_prompt(
    message: String,
    text_elements: Vec<TextElement>,
    local_image_paths: Vec<PathBuf>,
    remote_image_urls: Vec<String>,
) -> UserHistoryCell {
    UserHistoryCell {
        message,
        text_elements,
        local_image_paths,
        remote_image_urls,
    }
}

#[derive(Debug)]
pub(crate) struct CompositeHistoryCell {
    parts: Vec<Box<dyn HistoryCell>>,
}

impl CompositeHistoryCell {
    pub(crate) fn new(parts: Vec<Box<dyn HistoryCell>>) -> Self {
        Self { parts }
    }
}

impl HistoryCell for CompositeHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.display_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(Line::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn committed_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.committed_display_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(Line::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn transcript_animation_tick(&self) -> Option<u64> {
        self.parts
            .iter()
            .filter_map(|part| part.transcript_animation_tick())
            .max()
    }
}
