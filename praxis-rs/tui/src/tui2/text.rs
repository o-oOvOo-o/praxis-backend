use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::style::TextStyle;
use super::tokens::UiPalette;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextAtom<'a> {
    pub text: Cow<'a, str>,
    pub style: TextStyle,
}

impl<'a> TextAtom<'a> {
    pub(crate) fn new(text: impl Into<Cow<'a, str>>, style: TextStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextLineVisual<'a> {
    atoms: Vec<TextAtom<'a>>,
}

impl<'a> TextLineVisual<'a> {
    pub(crate) fn new(atoms: impl Into<Vec<TextAtom<'a>>>) -> Self {
        Self {
            atoms: atoms.into(),
        }
    }

    pub(crate) fn plain(text: impl Into<Cow<'a, str>>) -> Self {
        Self::new(vec![TextAtom::new(text, TextStyle::default())])
    }

    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer, palette: &UiPalette) {
        let spans = self
            .atoms
            .iter()
            .map(|atom| Span::styled(atom.text.clone(), atom.style.to_ratatui(palette)))
            .collect::<Vec<_>>();
        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::{TextAtom, TextLineVisual};
    use crate::tui2::{TextStyle, Tone, UiPalette};

    #[test]
    fn renders_semantic_foreground() {
        let palette = UiPalette::praxis_dark();
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 1));
        let line = TextLineVisual::new(vec![TextAtom::new("A", TextStyle::new(Tone::Accent))]);

        line.render(Rect::new(0, 0, 8, 1), &mut buf, &palette);

        assert_eq!(buf[(0, 0)].symbol(), "A");
        assert_eq!(buf[(0, 0)].fg, palette.accent);
    }
}
