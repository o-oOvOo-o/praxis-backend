use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::visual::VisualPalette;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::render::Insets;
use crate::render::RectExt as _;
use crate::tui2::UiPalette;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SurfaceThemeKind {
    Dark,
    Classic,
    DeepSeek,
}

impl SurfaceThemeKind {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Classic => "classic",
            Self::DeepSeek => "deepseek",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Classic => "Classic",
            Self::DeepSeek => "DeepSeek",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Dark => "Default dark Cursive-like Praxis surface",
            Self::Classic => "Dark IDE gray Cursive-like Praxis surface",
            Self::DeepSeek => "DeepSeek blue Cursive-like surface",
        }
    }

    fn from_id(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" | "default" | "common" => Some(Self::Dark),
            "classic" | "retro" | "cursive" => Some(Self::Classic),
            "deepseek" | "deep-seek" => Some(Self::DeepSeek),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceTheme {
    pub(crate) kind: SurfaceThemeKind,
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
    pub(crate) shadow_bg: Color,
    pub(crate) title_fg: Color,
}

pub(crate) const DARK: SurfaceTheme = SurfaceTheme {
    kind: SurfaceThemeKind::Dark,
    base_bg: Color::Rgb(8, 30, 63),
    panel_bg: Color::Rgb(15, 18, 24),
    panel_raised_bg: Color::Rgb(26, 31, 39),
    header_bg: Color::Rgb(19, 24, 32),
    footer_bg: Color::Rgb(13, 16, 22),
    input_bg: Color::Rgb(20, 25, 33),
    border_muted: Color::Rgb(94, 123, 156),
    gap_bg: Color::Rgb(5, 17, 38),
    gap_fg: Color::Rgb(96, 130, 172),
    text: Color::Rgb(226, 229, 234),
    text_strong: Color::Rgb(249, 250, 252),
    muted: Color::Rgb(148, 156, 168),
    dim: Color::Rgb(102, 111, 123),
    disabled: Color::Rgb(77, 84, 94),
    accent: Color::Rgb(132, 198, 148),
    active_bg: Color::Rgb(24, 42, 32),
    selected_bg: Color::Rgb(31, 43, 56),
    hover_bg: Color::Rgb(33, 37, 44),
    danger: Color::Rgb(219, 116, 104),
    control_bg: Color::Rgb(18, 32, 35),
    control_active_bg: Color::Rgb(23, 48, 42),
    control_selected_bg: Color::Rgb(24, 49, 60),
    control_hover_bg: Color::Rgb(29, 54, 59),
    control_accent: Color::Rgb(117, 196, 188),
    control_muted: Color::Rgb(142, 183, 181),
    chip_model_bg: Color::Rgb(25, 28, 33),
    chip_reasoning_bg: Color::Rgb(28, 29, 37),
    chip_rank_bg: Color::Rgb(24, 42, 32),
    chip_permission_bg: Color::Rgb(18, 32, 35),
    dropdown_bg: Color::Rgb(14, 16, 19),
    dropdown_current_bg: Color::Rgb(29, 47, 40),
    user_bubble_bg: Color::Rgb(27, 31, 36),
    shadow_bg: Color::Rgb(0, 0, 0),
    title_fg: Color::Rgb(132, 198, 148),
};

pub(crate) const CLASSIC: SurfaceTheme = SurfaceTheme {
    kind: SurfaceThemeKind::Classic,
    base_bg: Color::Rgb(18, 20, 23),
    panel_bg: Color::Rgb(25, 28, 32),
    panel_raised_bg: Color::Rgb(32, 35, 40),
    header_bg: Color::Rgb(32, 35, 40),
    footer_bg: Color::Rgb(32, 35, 40),
    input_bg: Color::Rgb(27, 30, 34),
    border_muted: Color::Rgb(88, 94, 103),
    gap_bg: Color::Rgb(25, 28, 32),
    gap_fg: Color::Rgb(92, 98, 108),
    text: Color::Rgb(226, 229, 234),
    text_strong: Color::Rgb(250, 251, 253),
    muted: Color::Rgb(170, 176, 185),
    dim: Color::Rgb(116, 123, 134),
    disabled: Color::Rgb(82, 88, 97),
    accent: Color::Rgb(142, 181, 235),
    active_bg: Color::Rgb(39, 44, 51),
    selected_bg: Color::Rgb(47, 76, 111),
    hover_bg: Color::Rgb(37, 41, 47),
    danger: Color::Rgb(228, 116, 112),
    control_bg: Color::Rgb(29, 32, 36),
    control_active_bg: Color::Rgb(39, 44, 51),
    control_selected_bg: Color::Rgb(47, 76, 111),
    control_hover_bg: Color::Rgb(38, 42, 48),
    control_accent: Color::Rgb(142, 181, 235),
    control_muted: Color::Rgb(141, 158, 181),
    chip_model_bg: Color::Rgb(29, 32, 36),
    chip_reasoning_bg: Color::Rgb(30, 33, 38),
    chip_rank_bg: Color::Rgb(34, 39, 45),
    chip_permission_bg: Color::Rgb(29, 32, 36),
    dropdown_bg: Color::Rgb(30, 33, 38),
    dropdown_current_bg: Color::Rgb(52, 82, 118),
    user_bubble_bg: Color::Rgb(32, 35, 40),
    shadow_bg: Color::Rgb(8, 9, 11),
    title_fg: Color::Rgb(142, 181, 235),
};

pub(crate) const DEEPSEEK: SurfaceTheme = SurfaceTheme {
    kind: SurfaceThemeKind::DeepSeek,
    base_bg: Color::Rgb(6, 10, 16),
    panel_bg: Color::Rgb(10, 17, 25),
    panel_raised_bg: Color::Rgb(13, 22, 32),
    header_bg: Color::Rgb(8, 15, 24),
    footer_bg: Color::Rgb(6, 12, 20),
    input_bg: Color::Rgb(8, 15, 24),
    border_muted: Color::Rgb(46, 78, 101),
    gap_bg: Color::Rgb(5, 9, 15),
    gap_fg: Color::Rgb(41, 66, 86),
    text: Color::Rgb(224, 238, 247),
    text_strong: Color::Rgb(244, 250, 253),
    muted: Color::Rgb(136, 161, 178),
    dim: Color::Rgb(72, 101, 122),
    disabled: Color::Rgb(69, 86, 98),
    accent: Color::Rgb(86, 178, 231),
    active_bg: Color::Rgb(13, 41, 63),
    selected_bg: Color::Rgb(19, 43, 61),
    hover_bg: Color::Rgb(24, 54, 76),
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
    shadow_bg: Color::Rgb(0, 3, 8),
    title_fg: Color::Rgb(108, 197, 231),
};

static RUNTIME_KIND: AtomicU8 = AtomicU8::new(SurfaceThemeKind::Classic as u8);

impl SurfaceTheme {
    pub(crate) fn visual_palette(self) -> VisualPalette {
        VisualPalette {
            base: self.base_bg,
            panel: self.panel_bg,
            raised: self.panel_raised_bg,
            hover: self.hover_bg,
            selected: self.selected_bg,
            active: self.active_bg,
            input: self.input_bg,
            text: self.text,
            text_strong: self.text_strong,
            text_muted: self.muted,
            text_inactive: self.disabled,
            text_inverse: self.base_bg,
            accent: self.accent,
            accent_soft: self.control_accent,
            control: self.control_bg,
            control_hover: self.control_hover_bg,
            control_selected: self.control_selected_bg,
            control_active: self.control_active_bg,
            control_accent: self.control_accent,
            border_muted: self.border_muted,
            danger: self.danger,
        }
    }

    pub(crate) fn ui_palette(self) -> UiPalette {
        UiPalette {
            text: self.text,
            text_strong: self.text_strong,
            text_muted: self.muted,
            text_inactive: self.disabled,
            text_inverse: self.base_bg,
            surface: self.base_bg,
            surface_panel: self.panel_bg,
            surface_raised: self.panel_raised_bg,
            surface_hover: self.hover_bg,
            surface_selected: self.selected_bg,
            surface_input: self.input_bg,
            selection_bg: self.control_selected_bg,
            accent: self.accent,
            accent_soft: self.control_accent,
            success: self.accent,
            warning: Color::Rgb(220, 178, 98),
            danger: self.danger,
            border: self.border_muted,
            border_focused: self.control_accent,
            diff_added: self.accent,
            diff_removed: self.danger,
            user_message_bg: self.user_bubble_bg,
            assistant_message_bg: self.panel_bg,
        }
    }
}

pub(crate) fn all_theme_kinds() -> &'static [SurfaceThemeKind] {
    &[
        SurfaceThemeKind::Dark,
        SurfaceThemeKind::Classic,
        SurfaceThemeKind::DeepSeek,
    ]
}

pub(crate) fn theme_for_kind(kind: SurfaceThemeKind) -> SurfaceTheme {
    match kind {
        SurfaceThemeKind::Dark => DARK,
        SurfaceThemeKind::Classic => CLASSIC,
        SurfaceThemeKind::DeepSeek => DEEPSEEK,
    }
}

pub(crate) fn resolve_kind(
    preference: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> SurfaceThemeKind {
    if let Some(value) = preference.map(str::trim).filter(|value| !value.is_empty())
        && !value.eq_ignore_ascii_case("auto")
        && let Some(kind) = SurfaceThemeKind::from_id(value)
    {
        return kind;
    }

    if is_deepseek(provider_id) || is_deepseek(model_label) {
        SurfaceThemeKind::DeepSeek
    } else {
        SurfaceThemeKind::Classic
    }
}

pub(crate) fn resolve_theme(
    preference: Option<&str>,
    provider_id: &str,
    model_label: &str,
) -> SurfaceTheme {
    theme_for_kind(resolve_kind(preference, provider_id, model_label))
}

pub(crate) fn set_runtime_theme_kind(kind: SurfaceThemeKind) {
    RUNTIME_KIND.store(kind as u8, Ordering::Relaxed);
}

pub(crate) fn runtime_theme_kind() -> SurfaceThemeKind {
    match RUNTIME_KIND.load(Ordering::Relaxed) {
        value if value == SurfaceThemeKind::Classic as u8 => SurfaceThemeKind::Classic,
        value if value == SurfaceThemeKind::DeepSeek as u8 => SurfaceThemeKind::DeepSeek,
        _ => SurfaceThemeKind::Classic,
    }
}

pub(crate) fn runtime_theme() -> SurfaceTheme {
    theme_for_kind(runtime_theme_kind())
}

pub(crate) fn render_menu_surface(area: Rect, buf: &mut Buffer) -> Rect {
    let theme = runtime_theme();
    let frame_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(1).max(1),
        height: area.height.saturating_sub(1).max(1),
    };
    render_popup_surface(frame_area, buf, theme, None);
    frame_area.inset(Insets::vh(1, 2))
}

pub(crate) fn render_panel_surface(
    area: Rect,
    buf: &mut Buffer,
    theme: SurfaceTheme,
    title: Option<Line<'static>>,
) {
    render_surface_shadow(area, buf.area, buf, theme);
    render_frame(area, buf, theme);
    render_title(area, buf, theme, title);
}

pub(crate) fn render_main_surface(
    area: Rect,
    buf: &mut Buffer,
    theme: SurfaceTheme,
    title: Option<Line<'static>>,
) {
    render_frame(area, buf, theme);
    render_title(area, buf, theme, title);
}

pub(crate) fn render_popup_surface(
    area: Rect,
    buf: &mut Buffer,
    theme: SurfaceTheme,
    title: Option<Line<'static>>,
) {
    render_surface_shadow(area, buf.area, buf, theme);
    render_box(area, buf, theme.dropdown_bg, theme.text, theme.border_muted);
    render_title_with_bg(area, buf, theme, theme.dropdown_bg, title);
}

pub(crate) fn render_input_surface(area: Rect, buf: &mut Buffer, theme: SurfaceTheme) {
    render_box(area, buf, theme.input_bg, theme.text, theme.border_muted);
}

pub(crate) fn render_panel_outline(
    area: Rect,
    buf: &mut Buffer,
    theme: SurfaceTheme,
    title: Option<Line<'static>>,
) {
    render_frame_outline(area, buf, theme);
    render_title(area, buf, theme, title);
}

pub(crate) fn selection_style(style: Style) -> Style {
    style
        .bg(runtime_theme().selected_bg)
        .fg(runtime_theme().text_strong)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn user_message_style() -> Style {
    let theme = runtime_theme();
    Style::default().fg(theme.text).bg(theme.user_bubble_bg)
}

fn render_surface_shadow(area: Rect, clip_area: Rect, buf: &mut Buffer, theme: SurfaceTheme) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    render_hard_shadow_edge(area, clip_area, buf, theme);
}

fn render_hard_shadow_edge(area: Rect, clip_area: Rect, buf: &mut Buffer, theme: SurfaceTheme) {
    let style = Style::default().bg(theme.shadow_bg).fg(theme.shadow_bg);
    let right = area.right();
    let bottom = area.bottom();
    for y in area.y.saturating_add(1)..bottom.saturating_add(1) {
        set_shadow_cell(buf, right, y, clip_area, style);
    }
    for x in area.x.saturating_add(1)..right.saturating_add(1) {
        set_shadow_cell(buf, x, bottom, clip_area, style);
    }
}

fn set_shadow_cell(buf: &mut Buffer, x: u16, y: u16, clip_area: Rect, style: Style) {
    if x < buf.area.x || y < buf.area.y || x >= buf.area.right() || y >= buf.area.bottom() {
        return;
    }
    if x < clip_area.x || y < clip_area.y || x >= clip_area.right() || y >= clip_area.bottom() {
        return;
    }
    buf[(x, y)].set_symbol(" ").set_style(style);
}

fn render_frame(area: Rect, buf: &mut Buffer, theme: SurfaceTheme) {
    render_box(area, buf, theme.panel_bg, theme.text, theme.border_muted);
}

fn render_box(area: Rect, buf: &mut Buffer, background: Color, foreground: Color, border: Color) {
    if area.is_empty() {
        return;
    }
    let fill = Style::default().bg(background).fg(foreground);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buf[(x, y)].set_symbol(" ").set_style(fill);
        }
    }
    if area.width < 2 || area.height < 2 {
        return;
    }
    let border = Style::default()
        .bg(background)
        .fg(border)
        .add_modifier(Modifier::BOLD);
    let left = area.x;
    let right = area.right().saturating_sub(1);
    let top = area.y;
    let bottom = area.bottom().saturating_sub(1);
    buf[(left, top)].set_symbol("╭").set_style(border);
    buf[(right, top)].set_symbol("╮").set_style(border);
    buf[(left, bottom)].set_symbol("╰").set_style(border);
    buf[(right, bottom)].set_symbol("╯").set_style(border);
    for x in left.saturating_add(1)..right {
        buf[(x, top)].set_symbol("─").set_style(border);
        buf[(x, bottom)].set_symbol("─").set_style(border);
    }
    for y in top.saturating_add(1)..bottom {
        buf[(left, y)].set_symbol("│").set_style(border);
        buf[(right, y)].set_symbol("│").set_style(border);
    }
}

fn render_frame_outline(area: Rect, buf: &mut Buffer, theme: SurfaceTheme) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let border = Style::default()
        .bg(theme.panel_bg)
        .fg(theme.border_muted)
        .add_modifier(Modifier::BOLD);
    let left = area.x;
    let right = area.right().saturating_sub(1);
    let top = area.y;
    let bottom = area.bottom().saturating_sub(1);
    buf[(left, top)].set_symbol("╭").set_style(border);
    buf[(right, top)].set_symbol("╮").set_style(border);
    buf[(left, bottom)].set_symbol("╰").set_style(border);
    buf[(right, bottom)].set_symbol("╯").set_style(border);
    for x in left.saturating_add(1)..right {
        buf[(x, top)].set_symbol("─").set_style(border);
        buf[(x, bottom)].set_symbol("─").set_style(border);
    }
    for y in top.saturating_add(1)..bottom {
        buf[(left, y)].set_symbol("│").set_style(border);
        buf[(right, y)].set_symbol("│").set_style(border);
    }
}

fn render_title(area: Rect, buf: &mut Buffer, theme: SurfaceTheme, title: Option<Line<'static>>) {
    render_title_with_bg(area, buf, theme, theme.panel_bg, title);
}

fn render_title_with_bg(
    area: Rect,
    buf: &mut Buffer,
    theme: SurfaceTheme,
    background: Color,
    title: Option<Line<'static>>,
) {
    let Some(title) = title else {
        return;
    };
    if area.width < 8 || area.height < 2 {
        return;
    }
    let title_area = Rect::new(
        area.x.saturating_add(2),
        area.y,
        area.width.saturating_sub(4),
        1,
    );
    Paragraph::new(title)
        .style(Style::default().fg(theme.title_fg).bg(background))
        .render(title_area, buf);
}

fn is_deepseek(value: &str) -> bool {
    value.to_ascii_lowercase().contains("deepseek")
}
