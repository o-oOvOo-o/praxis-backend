use ratatui::style::Color;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UiPalette {
    pub text: Color,
    pub text_strong: Color,
    pub text_muted: Color,
    pub text_inactive: Color,
    pub text_inverse: Color,
    pub surface: Color,
    pub surface_panel: Color,
    pub surface_raised: Color,
    pub surface_hover: Color,
    pub surface_selected: Color,
    pub surface_input: Color,
    pub selection_bg: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub border: Color,
    pub border_focused: Color,
    pub diff_added: Color,
    pub diff_removed: Color,
    pub user_message_bg: Color,
    pub assistant_message_bg: Color,
}

impl UiPalette {
    pub(crate) const PRAXIS_DARK: Self = Self {
        text: Color::Rgb(222, 226, 232),
        text_strong: Color::Rgb(250, 251, 252),
        text_muted: Color::Rgb(143, 153, 166),
        text_inactive: Color::Rgb(99, 108, 122),
        text_inverse: Color::Rgb(16, 18, 22),
        surface: Color::Rgb(13, 15, 18),
        surface_panel: Color::Rgb(20, 23, 28),
        surface_raised: Color::Rgb(29, 33, 39),
        surface_hover: Color::Rgb(36, 42, 50),
        surface_selected: Color::Rgb(47, 55, 66),
        surface_input: Color::Rgb(18, 21, 26),
        selection_bg: Color::Rgb(58, 91, 138),
        accent: Color::Rgb(95, 178, 255),
        accent_soft: Color::Rgb(88, 142, 198),
        success: Color::Rgb(94, 197, 123),
        warning: Color::Rgb(230, 181, 92),
        danger: Color::Rgb(232, 111, 111),
        border: Color::Rgb(62, 70, 82),
        border_focused: Color::Rgb(108, 179, 255),
        diff_added: Color::Rgb(70, 174, 112),
        diff_removed: Color::Rgb(225, 98, 98),
        user_message_bg: Color::Rgb(26, 38, 53),
        assistant_message_bg: Color::Rgb(18, 21, 26),
    };

    pub(crate) const fn praxis_dark() -> Self {
        Self::PRAXIS_DARK
    }
}
