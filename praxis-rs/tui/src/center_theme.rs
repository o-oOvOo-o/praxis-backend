use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CenterThemeKind {
    Common,
    DeepSeek,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CenterTheme {
    pub(crate) base_bg: Color,
    pub(crate) panel_bg: Color,
    pub(crate) panel_raised_bg: Color,
    pub(crate) header_bg: Color,
    pub(crate) footer_bg: Color,
    pub(crate) input_bg: Color,
    pub(crate) border_muted: Color,
    pub(crate) gap_bg: Color,
    pub(crate) gap_fg: Color,
    pub(crate) text: Color,
    pub(crate) text_strong: Color,
    pub(crate) muted: Color,
    pub(crate) dim: Color,
    pub(crate) disabled: Color,
    pub(crate) accent: Color,
    pub(crate) active_bg: Color,
    pub(crate) selected_bg: Color,
    pub(crate) hover_bg: Color,
    pub(crate) danger: Color,
    pub(crate) control_bg: Color,
    pub(crate) control_active_bg: Color,
    pub(crate) control_selected_bg: Color,
    pub(crate) control_hover_bg: Color,
    pub(crate) control_accent: Color,
    pub(crate) control_muted: Color,
    pub(crate) chip_model_bg: Color,
    pub(crate) chip_reasoning_bg: Color,
    pub(crate) chip_rank_bg: Color,
    pub(crate) chip_permission_bg: Color,
    pub(crate) dropdown_bg: Color,
    pub(crate) dropdown_current_bg: Color,
    pub(crate) user_bubble_bg: Color,
}

pub(crate) const COMMON: CenterTheme = CenterTheme {
    base_bg: Color::Rgb(8, 11, 11),
    panel_bg: Color::Rgb(15, 18, 17),
    panel_raised_bg: Color::Rgb(19, 23, 21),
    header_bg: Color::Rgb(11, 16, 17),
    footer_bg: Color::Rgb(8, 12, 13),
    input_bg: Color::Rgb(10, 15, 16),
    border_muted: Color::Rgb(47, 64, 59),
    gap_bg: Color::Rgb(7, 10, 10),
    gap_fg: Color::Rgb(50, 64, 58),
    text: Color::Rgb(226, 234, 228),
    text_strong: Color::Rgb(246, 250, 247),
    muted: Color::Rgb(139, 151, 143),
    dim: Color::Rgb(92, 106, 98),
    disabled: Color::Rgb(78, 88, 82),
    accent: Color::Rgb(112, 190, 132),
    active_bg: Color::Rgb(24, 39, 31),
    selected_bg: Color::Rgb(30, 40, 36),
    hover_bg: Color::Rgb(36, 49, 42),
    danger: Color::Rgb(190, 118, 96),
    control_bg: Color::Rgb(15, 34, 42),
    control_active_bg: Color::Rgb(18, 45, 52),
    control_selected_bg: Color::Rgb(20, 52, 61),
    control_hover_bg: Color::Rgb(25, 62, 70),
    control_accent: Color::Rgb(126, 184, 210),
    control_muted: Color::Rgb(152, 184, 190),
    chip_model_bg: Color::Rgb(25, 31, 30),
    chip_reasoning_bg: Color::Rgb(29, 31, 36),
    chip_rank_bg: Color::Rgb(24, 39, 31),
    chip_permission_bg: Color::Rgb(15, 34, 42),
    dropdown_bg: Color::Rgb(12, 17, 17),
    dropdown_current_bg: Color::Rgb(24, 42, 33),
    user_bubble_bg: Color::Rgb(32, 38, 36),
};

pub(crate) const DEEPSEEK: CenterTheme = CenterTheme {
    base_bg: Color::Rgb(6, 10, 16),
    panel_bg: Color::Rgb(10, 17, 25),
    panel_raised_bg: Color::Rgb(13, 22, 32),
    header_bg: Color::Rgb(8, 15, 24),
    footer_bg: Color::Rgb(6, 12, 20),
    input_bg: Color::Rgb(8, 15, 24),
    border_muted: Color::Rgb(39, 68, 88),
    gap_bg: Color::Rgb(5, 9, 15),
    gap_fg: Color::Rgb(41, 66, 86),
    text: Color::Rgb(224, 238, 247),
    text_strong: Color::Rgb(244, 250, 253),
    muted: Color::Rgb(136, 161, 178),
    dim: Color::Rgb(72, 101, 122),
    disabled: Color::Rgb(69, 86, 98),
    accent: Color::Rgb(80, 166, 216),
    active_bg: Color::Rgb(13, 41, 63),
    selected_bg: Color::Rgb(19, 39, 55),
    hover_bg: Color::Rgb(24, 50, 70),
    danger: Color::Rgb(205, 120, 110),
    control_bg: Color::Rgb(9, 48, 67),
    control_active_bg: Color::Rgb(11, 58, 82),
    control_selected_bg: Color::Rgb(14, 67, 92),
    control_hover_bg: Color::Rgb(17, 78, 106),
    control_accent: Color::Rgb(108, 197, 231),
    control_muted: Color::Rgb(150, 190, 207),
    chip_model_bg: Color::Rgb(14, 31, 45),
    chip_reasoning_bg: Color::Rgb(16, 35, 50),
    chip_rank_bg: Color::Rgb(13, 41, 63),
    chip_permission_bg: Color::Rgb(9, 48, 67),
    dropdown_bg: Color::Rgb(8, 17, 26),
    dropdown_current_bg: Color::Rgb(12, 47, 71),
    user_bubble_bg: Color::Rgb(24, 40, 52),
};

pub(crate) fn for_model(provider_id: &str, model_label: &str) -> CenterTheme {
    match kind_for_model(provider_id, model_label) {
        CenterThemeKind::Common => COMMON,
        CenterThemeKind::DeepSeek => DEEPSEEK,
    }
}

pub(crate) fn kind_for_model(provider_id: &str, model_label: &str) -> CenterThemeKind {
    if is_deepseek(provider_id) || is_deepseek(model_label) {
        CenterThemeKind::DeepSeek
    } else {
        CenterThemeKind::Common
    }
}

fn is_deepseek(value: &str) -> bool {
    value.to_ascii_lowercase().contains("deepseek")
}
