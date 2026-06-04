use std::time::Duration;

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

// Derived from Raifus MEGA_BANNER_SMALL_3 (MIT, sponkurtus2/Raifus).
const DEEPSEEK_THINKING_ART: &[&str] = &[
    "⠀⠀⠀⠀⠀⠀⠀⠀⢀⡠⠔⠂⠀⠀⠀⠀⠀⠀⠀⠀⠒⠠⢀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⡠⠖⠁⡴⠀⠔⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠢⡀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⢀⠊⠀⠀⠜⠀⡌⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠲⣄⠀⠀⠀",
    "⠀⠀⠀⢠⢃⠁⢀⡞⠀⢰⠀⠐⠀⠀⠀⠀⠀⠀⠀⠀⠘⡀⠀⠀⠀⠀⠘⣆⠀⠀",
    "⠀⠀⢠⡇⠁⠀⣾⠁⠀⣦⠀⠀⠀⠀⠀⠀⠀⡄⢀⠪⠵⠑⡄⠀⠀⠀⠀⢹⡄⠀",
    "⠀⠀⢆⠁⡄⢸⠹⡀⢀⠀⠀⢀⠀⠀⠀⠀⡀⢁⠈⢆⣀⣀⠕⠆⠀⠀⢃⠈⡇⠀",
    "⠀⢸⢸⠀⠀⡏⢩⢇⢈⢰⠀⠸⡰⠀⠀⢃⡇⠈⡆⠈⠀⠀⡆⠘⠄⠀⠈⠀⢇⠀",
    "⠀⠘⡔⡀⡀⣁⣴⣿⣼⠪⢦⣂⡱⣅⣢⣼⣶⣤⣴⡄⢸⠀⡧⣀⠂⡐⢄⣷⡼⠀",
    "⣠⢾⢿⣳⣷⣿⡟⠉⠛⡄⠀⠀⠀⠀⠘⠟⠉⢛⣿⣧⣸⠀⡏⠢⠝⢨⣿⣿⣷⠒",
    "⡅⢦⡌⣏⠉⣏⢀⣈⣡⠇⠀⠀⠀⠀⠀⣄⠈⢀⡸⣿⠙⠀⡿⠍⡄⢸⣧⠿⠘⢀",
    "⠫⠭⠊⣿⡄⢹⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠁⠀⣿⡄⣸⠩⢉⠟⡙⢏⠀⠓⣤",
    "⠀⠐⠖⠋⡼⡞⢄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⡿⣠⠇⢈⠠⠊⠀⠘⠧⠤⠄",
    "⠀⠀⠀⠀⠀⠹⡀⠑⠠⣀⠀⠀⠀⠀⠀⣀⠠⡰⣹⢃⠊⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠂⢄⠀⠀⠙⠓⠂⣀⠉⠤⠼⠯⠇⢑⣚⠚⠋⢑⠒⠈⠉⠁⠀⠈⠐⢄⠀⠀",
    "⠀⠀⠀⠀⠈⢢⠤⠊⠁⠀⠀⠀⠀⠀⠀⠀⠀⠈⠁⠃⠤⠀⠀⢀⠀⠉⠉⠁⢂⠀",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ThinkingPersona {
    #[default]
    None,
    CodexSummary,
    DeepSeekFull,
}

impl ThinkingPersona {
    pub(crate) fn is_visible(self) -> bool {
        !matches!(self, Self::None)
    }

    pub(crate) fn live_lines(self, width: u16, elapsed: Duration) -> Vec<Line<'static>> {
        if !self.is_visible() || width < 24 {
            return Vec::new();
        }

        let tick = (elapsed.as_millis() / 900) as u64;
        self.lines_for_tick(width, tick)
    }

    pub(crate) fn stable_lines(self, width: u16) -> Vec<Line<'static>> {
        if !self.is_visible() || width < 24 {
            return Vec::new();
        }
        self.lines_for_tick(width, 0)
    }

    pub(crate) fn desired_height(self, width: u16) -> u16 {
        self.stable_lines(width).len().try_into().unwrap_or(0)
    }

    fn lines_for_tick(self, width: u16, tick: u64) -> Vec<Line<'static>> {
        if self == Self::DeepSeekFull && width >= 64 {
            return deepseek_art_lines(width);
        }

        if width < 42 {
            return vec![self.compact_line(tick)];
        }

        let usable = usize::from(width).saturating_sub(2).min(80);
        let (title, pose, note, accent) = self.parts(tick);
        let top_prefix = format!("+-- {title} ");
        let dash_count = usable.saturating_sub(top_prefix.len()).max(1);
        let top = format!("{top_prefix}{}", "-".repeat(dash_count));
        let face = self.face(tick);
        let middle = clip_ascii(format!("|   {face}  {pose}"), usable);
        let bottom = clip_ascii(format!("+   {note}"), usable);

        vec![
            Line::from(vec![
                "  ".into(),
                Span::styled(top, Style::default().fg(accent)),
            ]),
            Line::from(vec![
                "  ".into(),
                Span::styled(middle, Style::default().fg(Color::Rgb(226, 232, 226))),
            ]),
            Line::from(vec![
                "  ".into(),
                Span::styled(
                    bottom,
                    Style::default().fg(Color::Rgb(143, 151, 145)).italic(),
                ),
            ]),
        ]
    }

    fn compact_line(self, tick: u64) -> Line<'static> {
        let (title, pose, _, accent) = self.parts(tick);
        Line::from(vec![
            "  ".into(),
            Span::styled(self.face(tick), Style::default().fg(accent)),
            " ".into(),
            Span::styled(title.to_string(), Style::default().fg(accent)),
            " ".into(),
            pose.dim(),
        ])
    }

    fn parts(self, tick: u64) -> (&'static str, &'static str, &'static str, Color) {
        match self {
            Self::None => ("", "", "", Color::Gray),
            Self::CodexSummary => (
                "Codex girl / summary only",
                "cool and brief",
                "public reasoning summary only",
                Color::Rgb(125, 211, 252),
            ),
            Self::DeepSeekFull => (
                "DeepSeek girl / thinking",
                if tick % 4 == 2 {
                    "chin in hand"
                } else {
                    "thinking quietly"
                },
                "raw thinking stream",
                Color::Rgb(104, 184, 126),
            ),
        }
    }

    fn face(self, tick: u64) -> &'static str {
        match self {
            Self::None => "",
            Self::CodexSummary => match tick % 6 {
                3 => "( -_-)",
                _ => "(-_-)",
            },
            Self::DeepSeekFull => match tick % 6 {
                1 | 2 => "(o.o)",
                3 => "(._.)",
                _ => "(. .)",
            },
        }
    }
}

fn clip_ascii(mut value: String, width: usize) -> String {
    if value.len() <= width {
        return value;
    }
    value.truncate(width);
    value
}

fn deepseek_art_lines(width: u16) -> Vec<Line<'static>> {
    let usable_width = usize::from(width).saturating_sub(2).max(1);
    let art_style = Style::default().fg(Color::Rgb(151, 160, 151));
    let mut lines = Vec::with_capacity(DEEPSEEK_THINKING_ART.len());
    for art_line in DEEPSEEK_THINKING_ART {
        let art_width = UnicodeWidthStr::width(*art_line);
        let pad = usable_width.saturating_sub(art_width) / 2;
        let mut spans = Vec::with_capacity(2);
        spans.push(Span::from(" ".repeat(pad)));
        spans.push(Span::styled(*art_line, art_style));
        lines.push(Line::from(spans));
    }
    lines
}
