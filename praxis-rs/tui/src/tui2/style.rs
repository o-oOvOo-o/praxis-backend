use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;

use super::tokens::UiPalette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Normal,
    Strong,
    Muted,
    Inactive,
    Inverse,
    Accent,
    AccentSoft,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Transparent,
    Base,
    Panel,
    Raised,
    Hover,
    Selected,
    Input,
    Selection,
    UserMessage,
    AssistantMessage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextEmphasis {
    Plain,
    Bold,
    Italic,
    Underline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextStyle {
    pub tone: Tone,
    pub surface: Surface,
    pub emphasis: TextEmphasis,
}

impl TextStyle {
    pub(crate) const fn new(tone: Tone) -> Self {
        Self {
            tone,
            surface: Surface::Transparent,
            emphasis: TextEmphasis::Plain,
        }
    }

    pub(crate) const fn strong() -> Self {
        Self::new(Tone::Strong)
    }

    pub(crate) const fn muted() -> Self {
        Self::new(Tone::Muted)
    }

    pub(crate) const fn inactive() -> Self {
        Self::new(Tone::Inactive)
    }

    pub(crate) const fn accent() -> Self {
        Self::new(Tone::Accent)
    }

    pub(crate) const fn with_surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub(crate) const fn with_emphasis(mut self, emphasis: TextEmphasis) -> Self {
        self.emphasis = emphasis;
        self
    }

    pub(crate) fn to_ratatui(self, palette: &UiPalette) -> Style {
        let mut style = Style::default().fg(self.tone.color(palette));
        if let Some(bg) = self.surface.color(palette) {
            style = style.bg(bg);
        }

        match self.emphasis {
            TextEmphasis::Plain => style,
            TextEmphasis::Bold => style.add_modifier(Modifier::BOLD),
            TextEmphasis::Italic => style.add_modifier(Modifier::ITALIC),
            TextEmphasis::Underline => style.add_modifier(Modifier::UNDERLINED),
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new(Tone::Normal)
    }
}

impl Tone {
    fn color(self, palette: &UiPalette) -> Color {
        match self {
            Tone::Normal => palette.text,
            Tone::Strong => palette.text_strong,
            Tone::Muted => palette.text_muted,
            Tone::Inactive => palette.text_inactive,
            Tone::Inverse => palette.text_inverse,
            Tone::Accent => palette.accent,
            Tone::AccentSoft => palette.accent_soft,
            Tone::Success => palette.success,
            Tone::Warning => palette.warning,
            Tone::Danger => palette.danger,
        }
    }
}

impl Surface {
    pub(crate) fn to_ratatui(self, palette: &UiPalette) -> Style {
        let mut style = Style::default();
        if let Some(bg) = self.color(palette) {
            style = style.bg(bg);
        }
        style
    }

    fn color(self, palette: &UiPalette) -> Option<Color> {
        match self {
            Surface::Transparent => None,
            Surface::Base => Some(palette.surface),
            Surface::Panel => Some(palette.surface_panel),
            Surface::Raised => Some(palette.surface_raised),
            Surface::Hover => Some(palette.surface_hover),
            Surface::Selected => Some(palette.surface_selected),
            Surface::Input => Some(palette.surface_input),
            Surface::Selection => Some(palette.selection_bg),
            Surface::UserMessage => Some(palette.user_message_bg),
            Surface::AssistantMessage => Some(palette.assistant_message_bg),
        }
    }
}
