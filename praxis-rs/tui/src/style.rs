use crate::color::blend;
use crate::color::is_light;
use crate::terminal_palette::TerminalAppearance;
use crate::terminal_palette::TerminalTheme;
use crate::terminal_palette::best_color;
use crate::terminal_palette::best_color_with_ansi_fallback;
use crate::terminal_palette::default_bg;
use crate::terminal_palette::terminal_appearance;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;

const LIGHT_SELECTION_BG_RGB: (u8, u8, u8) = (180, 213, 255);
const DARK_SELECTION_BG_RGB: (u8, u8, u8) = (38, 79, 120);
const LIGHT_INTERACTIVE_SURFACE_BG_RGB: (u8, u8, u8) = (232, 236, 244);
const DARK_INTERACTIVE_SURFACE_BG_RGB: (u8, u8, u8) = (44, 50, 62);
const LIGHT_SEARCH_HIGHLIGHT_BG_RGB: (u8, u8, u8) = (255, 236, 179);
const DARK_SEARCH_HIGHLIGHT_BG_RGB: (u8, u8, u8) = (120, 90, 36);
const LIGHT_USER_MESSAGE_BG_ALPHA: f32 = 0.08;
const DARK_USER_MESSAGE_BG_ALPHA: f32 = 0.18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SemanticTheme {
    pub(crate) appearance: TerminalAppearance,
    pub(crate) user_message_bg_rgb: Option<(u8, u8, u8)>,
    pub(crate) interactive_surface_bg_rgb: Option<(u8, u8, u8)>,
    pub(crate) selection_bg_rgb: (u8, u8, u8),
    pub(crate) search_highlight_bg_rgb: (u8, u8, u8),
}

pub(crate) fn semantic_theme() -> SemanticTheme {
    semantic_theme_for(terminal_appearance())
}

pub(crate) fn semantic_theme_for(appearance: TerminalAppearance) -> SemanticTheme {
    SemanticTheme {
        user_message_bg_rgb: appearance.bg.map(user_message_bg_rgb),
        interactive_surface_bg_rgb: Some(interactive_surface_bg_rgb(
            appearance.theme,
            appearance.bg,
        )),
        selection_bg_rgb: selection_bg_rgb(appearance.theme),
        search_highlight_bg_rgb: search_highlight_bg_rgb(appearance.theme, appearance.bg),
        appearance,
    }
}

pub fn user_message_style() -> Style {
    user_message_style_for(default_bg())
}

pub(crate) fn user_message_rule_style() -> Style {
    user_message_rule_style_for(default_bg())
}

fn user_message_rule_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    user_message_style_for(terminal_bg)
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD)
}

pub fn proposed_plan_style() -> Style {
    proposed_plan_style_for(default_bg())
}

/// Returns the style for a user-authored message using the provided terminal background.
pub fn user_message_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    match terminal_bg.map(user_message_bg) {
        Some(bg) => Style::default().bg(bg),
        None => Style::default(),
    }
}

pub fn proposed_plan_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    match terminal_bg.map(proposed_plan_bg) {
        Some(bg) => Style::default().bg(bg),
        None => Style::default(),
    }
}

pub(crate) fn interactive_surface_style() -> Style {
    match semantic_theme()
        .interactive_surface_bg_rgb
        .map(surface_bg_color)
    {
        Some(bg) => Style::default().bg(bg),
        None => Style::default(),
    }
}

pub(crate) fn interactive_badge_style() -> Style {
    interactive_surface_style().add_modifier(Modifier::BOLD)
}

pub(crate) fn selection_style() -> Style {
    let theme = semantic_theme();
    Style::default()
        .bg(selection_bg_color(&theme))
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn selection_overlay(style: Style) -> Style {
    let theme = semantic_theme();
    overlay_with_bg(style, selection_bg_color(&theme), /*bold*/ true)
}

pub(crate) fn search_highlight_style() -> Style {
    let theme = semantic_theme();
    Style::default()
        .bg(search_highlight_bg_color(&theme))
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn search_highlight_overlay(style: Style) -> Style {
    let theme = semantic_theme();
    overlay_with_bg(style, search_highlight_bg_color(&theme), /*bold*/ true)
}

#[allow(clippy::disallowed_methods)]
pub fn user_message_bg(terminal_bg: (u8, u8, u8)) -> Color {
    surface_bg_color(user_message_bg_rgb(terminal_bg))
}

#[allow(clippy::disallowed_methods)]
pub fn proposed_plan_bg(terminal_bg: (u8, u8, u8)) -> Color {
    user_message_bg(terminal_bg)
}

fn user_message_bg_rgb(terminal_bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let (top, alpha) = if is_light(terminal_bg) {
        ((0, 0, 0), LIGHT_USER_MESSAGE_BG_ALPHA)
    } else {
        ((255, 255, 255), DARK_USER_MESSAGE_BG_ALPHA)
    };
    blend(top, terminal_bg, alpha)
}

fn interactive_surface_bg_rgb(
    theme: TerminalTheme,
    terminal_bg: Option<(u8, u8, u8)>,
) -> (u8, u8, u8) {
    match (theme, terminal_bg) {
        (TerminalTheme::Light, Some(bg)) => blend((87, 105, 247), bg, 0.12),
        (TerminalTheme::Dark, Some(bg)) => blend((126, 146, 255), bg, 0.26),
        (TerminalTheme::Light, None) => LIGHT_INTERACTIVE_SURFACE_BG_RGB,
        (TerminalTheme::Dark, None) => DARK_INTERACTIVE_SURFACE_BG_RGB,
    }
}

fn selection_bg_rgb(theme: TerminalTheme) -> (u8, u8, u8) {
    match theme {
        TerminalTheme::Light => LIGHT_SELECTION_BG_RGB,
        TerminalTheme::Dark => DARK_SELECTION_BG_RGB,
    }
}

fn search_highlight_bg_rgb(
    theme: TerminalTheme,
    terminal_bg: Option<(u8, u8, u8)>,
) -> (u8, u8, u8) {
    match (theme, terminal_bg) {
        (TerminalTheme::Light, Some(bg)) => blend((255, 196, 61), bg, 0.34),
        (TerminalTheme::Dark, Some(bg)) => blend((245, 171, 53), bg, 0.40),
        (TerminalTheme::Light, None) => LIGHT_SEARCH_HIGHLIGHT_BG_RGB,
        (TerminalTheme::Dark, None) => DARK_SEARCH_HIGHLIGHT_BG_RGB,
    }
}

fn surface_bg_color(rgb: (u8, u8, u8)) -> Color {
    best_color(rgb)
}

fn selection_bg_color(theme: &SemanticTheme) -> Color {
    let ansi_fallback = match theme.appearance.theme {
        TerminalTheme::Light => Color::Cyan,
        TerminalTheme::Dark => Color::Blue,
    };
    best_color_with_ansi_fallback(theme.selection_bg_rgb, ansi_fallback)
}

fn search_highlight_bg_color(theme: &SemanticTheme) -> Color {
    best_color_with_ansi_fallback(theme.search_highlight_bg_rgb, Color::Yellow)
}

fn overlay_with_bg(style: Style, bg: Color, bold: bool) -> Style {
    let style = style.bg(bg);
    if bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_palette::TerminalAppearance;
    use crate::terminal_palette::TerminalThemeSource;
    use pretty_assertions::assert_eq;
    use ratatui::style::Color;

    fn appearance(theme: TerminalTheme, bg: Option<(u8, u8, u8)>) -> TerminalAppearance {
        TerminalAppearance {
            fg: None,
            bg,
            theme,
            source: TerminalThemeSource::FallbackDark,
        }
    }

    #[test]
    fn semantic_theme_uses_claude_code_selection_bg_per_terminal_theme() {
        let light = semantic_theme_for(appearance(TerminalTheme::Light, /*bg*/ None));
        let dark = semantic_theme_for(appearance(TerminalTheme::Dark, /*bg*/ None));

        assert_eq!(light.selection_bg_rgb, LIGHT_SELECTION_BG_RGB);
        assert_eq!(dark.selection_bg_rgb, DARK_SELECTION_BG_RGB);
    }

    #[test]
    fn semantic_theme_derives_search_highlight_from_terminal_background() {
        let theme = semantic_theme_for(appearance(TerminalTheme::Dark, Some((24, 24, 27))));

        assert_eq!(
            theme.search_highlight_bg_rgb,
            blend((245, 171, 53), (24, 24, 27), 0.40)
        );
    }

    #[test]
    fn selection_overlay_preserves_existing_foreground() {
        let base = Style::default().fg(Color::Red);
        let selected = selection_overlay(base);

        assert_eq!(selected.fg, Some(Color::Red));
        assert!(selected.bg.is_some());
        assert!(selected.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn interactive_surface_fallback_tracks_terminal_theme() {
        let light = semantic_theme_for(appearance(TerminalTheme::Light, /*bg*/ None));
        let dark = semantic_theme_for(appearance(TerminalTheme::Dark, /*bg*/ None));

        assert_eq!(
            light.interactive_surface_bg_rgb,
            Some(LIGHT_INTERACTIVE_SURFACE_BG_RGB)
        );
        assert_eq!(
            dark.interactive_surface_bg_rgb,
            Some(DARK_INTERACTIVE_SURFACE_BG_RGB)
        );
    }

    #[test]
    fn user_message_background_has_visible_contrast() {
        assert_eq!(
            user_message_bg_rgb((240, 240, 240)),
            blend((0, 0, 0), (240, 240, 240), LIGHT_USER_MESSAGE_BG_ALPHA)
        );
        assert_eq!(
            user_message_bg_rgb((20, 20, 20)),
            blend((255, 255, 255), (20, 20, 20), DARK_USER_MESSAGE_BG_ALPHA)
        );
    }

    #[test]
    fn user_message_rule_keeps_panel_background() {
        let rule = user_message_rule_style_for(Some((20, 20, 20)));

        assert_eq!(rule.fg, Some(Color::DarkGray));
        assert!(rule.bg.is_some());
        assert!(rule.add_modifier.contains(Modifier::BOLD));
    }
}
