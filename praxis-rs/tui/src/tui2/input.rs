use std::borrow::Cow;
use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::style::{Surface, TextStyle, Tone};
use super::tokens::UiPalette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CursorPosition {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InputVisual<'a> {
    pub value: Cow<'a, str>,
    pub placeholder: Cow<'a, str>,
    pub prefix: Cow<'a, str>,
    pub cursor_byte: usize,
    pub focused: bool,
    pub selection: Option<Range<usize>>,
}

impl<'a> InputVisual<'a> {
    pub(crate) fn new(value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            value: value.into(),
            placeholder: Cow::Borrowed(""),
            prefix: Cow::Borrowed(""),
            cursor_byte: 0,
            focused: false,
            selection: None,
        }
    }

    pub(crate) fn placeholder(mut self, placeholder: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub(crate) fn prefix(mut self, prefix: impl Into<Cow<'a, str>>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub(crate) fn cursor_byte(mut self, cursor_byte: usize) -> Self {
        self.cursor_byte = cursor_byte;
        self
    }

    pub(crate) fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub(crate) fn selection(mut self, selection: Option<Range<usize>>) -> Self {
        self.selection = selection;
        self
    }

    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        palette: &UiPalette,
    ) -> Option<CursorPosition> {
        if area.is_empty() {
            return None;
        }

        buf.set_style(area, Surface::Input.to_ratatui(palette));
        let mut spans = Vec::new();

        if !self.prefix.is_empty() {
            spans.push(Span::styled(
                self.prefix.clone(),
                TextStyle::new(Tone::AccentSoft)
                    .with_surface(Surface::Input)
                    .to_ratatui(palette),
            ));
        }

        if self.value.is_empty() {
            spans.push(Span::styled(
                self.placeholder.clone(),
                TextStyle::inactive()
                    .with_surface(Surface::Input)
                    .to_ratatui(palette),
            ));
        } else if let Some(selection) = self.normalized_selection() {
            let (before, selected, after) = split_three(&self.value, selection);
            push_value_span(&mut spans, before, Surface::Input, palette);
            push_value_span(&mut spans, selected, Surface::Selection, palette);
            push_value_span(&mut spans, after, Surface::Input, palette);
        } else {
            push_value_span(&mut spans, &self.value, Surface::Input, palette);
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
        self.cursor_position(area)
    }

    fn cursor_position(&self, area: Rect) -> Option<CursorPosition> {
        if !self.focused || area.is_empty() {
            return None;
        }

        let cursor_byte = clamp_char_boundary(&self.value, self.cursor_byte);
        let value_before_cursor = &self.value[..cursor_byte];
        let cursor_offset = self
            .prefix
            .width()
            .saturating_add(value_before_cursor.width());
        let x_offset = cursor_offset.min(usize::from(area.width.saturating_sub(1))) as u16;

        Some(CursorPosition {
            x: area.x.saturating_add(x_offset),
            y: area.y,
        })
    }

    fn normalized_selection(&self) -> Option<Range<usize>> {
        let selection = self.selection.clone()?;
        let start = clamp_char_boundary(&self.value, selection.start.min(self.value.len()));
        let end = clamp_char_boundary(&self.value, selection.end.min(self.value.len()));
        if start == end {
            None
        } else if start < end {
            Some(start..end)
        } else {
            Some(end..start)
        }
    }
}

fn push_value_span<'a>(
    spans: &mut Vec<Span<'a>>,
    text: &'a str,
    surface: Surface,
    palette: &UiPalette,
) {
    if text.is_empty() {
        return;
    }

    spans.push(Span::styled(
        text.to_owned(),
        TextStyle::new(Tone::Normal)
            .with_surface(surface)
            .to_ratatui(palette),
    ));
}

fn split_three(value: &str, selection: Range<usize>) -> (&str, &str, &str) {
    (
        &value[..selection.start],
        &value[selection.start..selection.end],
        &value[selection.end..],
    )
}

fn clamp_char_boundary(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::InputVisual;
    use crate::tui2::UiPalette;

    #[test]
    fn empty_value_renders_placeholder_as_inactive_text() {
        let palette = UiPalette::praxis_dark();
        let mut buf = Buffer::empty(Rect::new(0, 0, 16, 1));

        InputVisual::new("").placeholder("message").render(
            Rect::new(0, 0, 16, 1),
            &mut buf,
            &palette,
        );

        assert_eq!(buf[(0, 0)].symbol(), "m");
        assert_eq!(buf[(0, 0)].fg, palette.text_inactive);
        assert_eq!(buf[(0, 0)].bg, palette.surface_input);
    }

    #[test]
    fn cursor_accounts_for_prefix_and_wide_text() {
        let cursor = InputVisual::new("你a")
            .prefix("> ")
            .cursor_byte("你".len())
            .focused(true)
            .render(
                Rect::new(4, 2, 20, 1),
                &mut Buffer::empty(Rect::new(4, 2, 20, 1)),
                &UiPalette::praxis_dark(),
            )
            .expect("focused input should return cursor");

        assert_eq!(cursor.x, 8);
        assert_eq!(cursor.y, 2);
    }
}
