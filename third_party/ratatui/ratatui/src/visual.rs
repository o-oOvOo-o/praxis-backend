//! Opinionated visual primitives for dense terminal applications.

use crate::buffer::Buffer;
use crate::layout::Rect;
use crate::style::{Color, Modifier, Style};

/// A compact dark palette for application chrome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisualPalette {
    /// The root background.
    pub base: Color,
    /// The default panel background.
    pub panel: Color,
    /// The raised panel background.
    pub raised: Color,
    /// The hover surface background.
    pub hover: Color,
    /// The selected surface background.
    pub selected: Color,
    /// The active surface background.
    pub active: Color,
    /// The input surface background.
    pub input: Color,
    /// The normal text color.
    pub text: Color,
    /// The strong text color.
    pub text_strong: Color,
    /// The muted text color.
    pub text_muted: Color,
    /// The inactive text color.
    pub text_inactive: Color,
    /// The inverse text color.
    pub text_inverse: Color,
    /// The primary accent color.
    pub accent: Color,
    /// The subtle accent color.
    pub accent_soft: Color,
    /// The controlled surface background.
    pub control: Color,
    /// The controlled hover background.
    pub control_hover: Color,
    /// The controlled selected background.
    pub control_selected: Color,
    /// The controlled active background.
    pub control_active: Color,
    /// The controlled accent color.
    pub control_accent: Color,
    /// The muted border color.
    pub border_muted: Color,
    /// The danger color.
    pub danger: Color,
}

impl VisualPalette {
    /// Returns a neutral compact dark palette.
    #[must_use]
    pub const fn compact_dark() -> Self {
        Self {
            base: Color::Rgb(9, 10, 12),
            panel: Color::Rgb(16, 17, 20),
            raised: Color::Rgb(25, 27, 31),
            hover: Color::Rgb(31, 34, 40),
            selected: Color::Rgb(30, 39, 51),
            active: Color::Rgb(24, 38, 30),
            input: Color::Rgb(20, 22, 26),
            text: Color::Rgb(226, 229, 234),
            text_strong: Color::Rgb(249, 250, 252),
            text_muted: Color::Rgb(145, 152, 162),
            text_inactive: Color::Rgb(77, 84, 94),
            text_inverse: Color::Rgb(9, 10, 12),
            accent: Color::Rgb(122, 187, 137),
            accent_soft: Color::Rgb(111, 184, 178),
            control: Color::Rgb(18, 30, 33),
            control_hover: Color::Rgb(28, 49, 54),
            control_selected: Color::Rgb(24, 45, 54),
            control_active: Color::Rgb(23, 44, 39),
            control_accent: Color::Rgb(111, 184, 178),
            border_muted: Color::Rgb(52, 57, 65),
            danger: Color::Rgb(219, 116, 104),
        }
    }

    /// Returns the surface style for an interactive row.
    #[must_use]
    pub fn row_style(self, state: InteractiveState) -> Style {
        let bg = if state.controlled && state.active {
            self.control_active
        } else if state.controlled && state.hovered {
            self.control_hover
        } else if state.controlled && state.selected {
            self.control_selected
        } else if state.controlled {
            self.control
        } else if state.active {
            self.active
        } else if state.hovered {
            self.hover
        } else if state.selected {
            self.selected
        } else {
            self.panel
        };
        let fg = if state.active || state.selected || state.hovered {
            self.text_strong
        } else {
            self.text
        };
        Style::default().bg(bg).fg(fg)
    }

    /// Returns the accent color for an interactive row.
    #[must_use]
    pub fn row_accent(self, state: InteractiveState) -> Option<Color> {
        if state.controlled {
            Some(self.control_accent)
        } else if state.active {
            Some(self.accent)
        } else if state.selected {
            Some(self.accent_soft)
        } else if state.hovered {
            Some(self.border_muted)
        } else {
            None
        }
    }

    /// Returns a compact status style.
    #[must_use]
    pub fn status_style(self, state: InteractiveState, color: Color) -> Style {
        let bg = if state.controlled {
            self.control
        } else {
            self.raised
        };
        Style::default().bg(bg).fg(color).add_modifier(Modifier::BOLD)
    }

    /// Returns a compact button style.
    #[must_use]
    pub fn button_style(self, hovered: bool, active: bool) -> Style {
        if active {
            Style::default()
                .bg(self.active)
                .fg(self.text_strong)
                .add_modifier(Modifier::BOLD)
        } else if hovered {
            Style::default().bg(self.hover).fg(self.text_strong)
        } else {
            Style::default().bg(self.raised).fg(self.accent)
        }
    }

    /// Returns the input surface color for focus and hover states.
    #[must_use]
    pub fn input_surface(self, focused: bool, hovered: bool) -> Color {
        if focused {
            self.active
        } else if hovered {
            self.hover
        } else {
            self.input
        }
    }
}

impl Default for VisualPalette {
    fn default() -> Self {
        Self::compact_dark()
    }
}

/// Interactive state used by visual primitives.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InteractiveState {
    /// Whether the item is active.
    pub active: bool,
    /// Whether the item is selected.
    pub selected: bool,
    /// Whether the item is hovered.
    pub hovered: bool,
    /// Whether the item is under explicit control.
    pub controlled: bool,
}

impl InteractiveState {
    /// Creates a new interactive state.
    #[must_use]
    pub const fn new(active: bool, selected: bool, hovered: bool, controlled: bool) -> Self {
        Self {
            active,
            selected,
            hovered,
            controlled,
        }
    }
}

/// Renders a slim row accent bar.
pub fn render_accent_bar(buf: &mut Buffer, area: Rect, style: Style, color: Option<Color>) {
    let Some(color) = color else {
        return;
    };
    if area.is_empty() {
        return;
    }
    for line in 0..area.height {
        buf[(area.x, area.y + line)]
            .set_symbol("│")
            .set_style(style.fg(color).add_modifier(Modifier::BOLD));
    }
}

/// An open horizontal frame for composer and status surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenFrame {
    /// The filled background color.
    pub background: Color,
    /// The horizontal rule color.
    pub border: Color,
    /// Whether to draw the top rule.
    pub top: bool,
    /// Whether to draw the bottom rule.
    pub bottom: bool,
}

impl OpenFrame {
    /// Creates a horizontal frame with explicit top and bottom edges.
    #[must_use]
    pub const fn horizontal(background: Color, border: Color, top: bool, bottom: bool) -> Self {
        Self {
            background,
            border,
            top,
            bottom,
        }
    }

    /// Renders the frame into the target area.
    pub fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let fill = Style::default().bg(self.background);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].set_symbol(" ").set_style(fill);
            }
        }

        let rule = fill.fg(self.border);
        if self.top {
            self.render_rule(area.y, area, buf, rule);
        }
        if self.bottom {
            let bottom = area.bottom().saturating_sub(1);
            if !self.top || bottom != area.y {
                self.render_rule(bottom, area, buf, rule);
            }
        }
    }

    fn render_rule(self, y: u16, area: Rect, buf: &mut Buffer, style: Style) {
        for x in area.x..area.right() {
            buf[(x, y)].set_symbol("─").set_style(style);
        }
    }
}
