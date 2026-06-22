use std::time::Duration;

use ratatui::text::Line;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ThinkingPersona {
    #[default]
    None,
    PraxisSummary,
    DeepSeekFull,
}

impl ThinkingPersona {
    pub(crate) fn is_visible(self) -> bool {
        let _ = self;
        false
    }

    pub(crate) fn live_lines(self, _width: u16, _elapsed: Duration) -> Vec<Line<'static>> {
        let _ = self;
        Vec::new()
    }

    pub(crate) fn desired_height(self, _width: u16) -> u16 {
        let _ = self;
        0
    }
}
