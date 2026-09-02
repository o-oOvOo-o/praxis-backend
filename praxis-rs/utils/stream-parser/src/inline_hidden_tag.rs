use crate::TextProjection;
use crate::TextProjector;
use crate::literal_matcher::LiteralMatcher;
use crate::literal_matcher::partial_suffix_len;

/// One hidden inline tag extracted by [`HiddenTagProjector`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedInlineTag<T> {
    pub tag: T,
    pub content: String,
}

/// Literal tag specification used by [`HiddenTagProjector`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineTagSpec<T> {
    pub tag: T,
    pub open: &'static str,
    pub close: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveTag<T> {
    tag: T,
    close: &'static str,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseMode<T> {
    Visible,
    Hidden(ActiveTag<T>),
}

/// Generic streaming parser that hides configured inline tags and extracts their contents.
///
/// Example:
/// - input: `hello <praxis-memory-citation>doc A</praxis-memory-citation> world`
/// - visible output: `hello  world`
/// - extracted: `["doc A"]`
///
/// Matching is literal and non-nested. If EOF is reached while a tag is still open, the parser
/// auto-closes it and returns the buffered content as extracted data.
#[derive(Debug)]
pub struct HiddenTagProjector<T>
where
    T: Clone + Eq,
{
    specs: Box<[InlineTagSpec<T>]>,
    openers: LiteralMatcher<usize>,
    pending: String,
    mode: ParseMode<T>,
}

impl<T> HiddenTagProjector<T>
where
    T: Clone + Eq,
{
    /// Create a parser for one or more hidden inline tags.
    pub fn new(specs: Vec<InlineTagSpec<T>>) -> Self {
        assert!(
            !specs.is_empty(),
            "HiddenTagProjector requires at least one tag spec"
        );
        for spec in &specs {
            assert!(
                !spec.open.is_empty(),
                "HiddenTagProjector requires non-empty open delimiters"
            );
            assert!(
                !spec.close.is_empty(),
                "HiddenTagProjector requires non-empty close delimiters"
            );
        }
        let openers = LiteralMatcher::new(
            specs
                .iter()
                .enumerate()
                .map(|(index, spec)| (spec.open, index)),
        );
        Self {
            specs: specs.into_boxed_slice(),
            openers,
            pending: String::new(),
            mode: ParseMode::Visible,
        }
    }

    fn drain_visible(&mut self, len: usize, out: &mut TextProjection<ExtractedInlineTag<T>>) {
        out.renderable.push_str(&self.pending[..len]);
        self.pending.drain(..len);
    }

    fn consume_hidden(&mut self, out: &mut TextProjection<ExtractedInlineTag<T>>) -> bool {
        let ParseMode::Hidden(active) = &self.mode else {
            return false;
        };
        if let Some(close_at) = self.pending.find(active.close) {
            let prior = std::mem::replace(&mut self.mode, ParseMode::Visible);
            let ParseMode::Hidden(mut active) = prior else {
                return false;
            };
            active.content.push_str(&self.pending[..close_at]);
            self.pending.drain(..close_at + active.close.len());
            out.events.push(ExtractedInlineTag {
                tag: active.tag,
                content: active.content,
            });
            return true;
        }

        let keep = partial_suffix_len(&self.pending, active.close);
        let drain = self.pending.len().saturating_sub(keep);
        if drain > 0 {
            let content = &self.pending[..drain];
            if let ParseMode::Hidden(active) = &mut self.mode {
                active.content.push_str(content);
            }
            self.pending.drain(..drain);
        }
        false
    }
}

impl<T> TextProjector for HiddenTagProjector<T>
where
    T: Clone + Eq,
{
    type Event = ExtractedInlineTag<T>;

    fn project(&mut self, input: &str) -> TextProjection<Self::Event> {
        self.pending.push_str(input);
        let mut out = TextProjection::default();

        loop {
            if matches!(self.mode, ParseMode::Hidden(_)) {
                if self.consume_hidden(&mut out) {
                    continue;
                }
                break;
            }

            if let Some((open_at, spec_index)) = self
                .openers
                .earliest(&self.pending)
                .map(|(offset, index)| (offset, *index))
            {
                let candidate = &self.pending[open_at..];
                if self.openers.could_extend_match(candidate) {
                    self.drain_visible(open_at, &mut out);
                    break;
                }
                self.drain_visible(open_at, &mut out);
                let spec = &self.specs[spec_index];
                let open_len = spec.open.len();
                self.pending.drain(..open_len);
                self.mode = ParseMode::Hidden(ActiveTag {
                    tag: spec.tag.clone(),
                    close: spec.close,
                    content: String::new(),
                });
                continue;
            }

            let keep = self.openers.retained_suffix_len(&self.pending);
            self.drain_visible(self.pending.len().saturating_sub(keep), &mut out);
            break;
        }

        out
    }

    fn close(&mut self) -> TextProjection<Self::Event> {
        let mut out = TextProjection::default();

        if let ParseMode::Hidden(mut active) = std::mem::replace(&mut self.mode, ParseMode::Visible)
        {
            if !self.pending.is_empty() {
                active.content.push_str(&self.pending);
                self.pending.clear();
            }
            out.events.push(ExtractedInlineTag {
                tag: active.tag,
                content: active.content,
            });
            return out;
        }

        if !self.pending.is_empty() {
            out.renderable.push_str(&self.pending);
            self.pending.clear();
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::HiddenTagProjector;
    use super::InlineTagSpec;
    use crate::TextProjection;
    use crate::TextProjector;
    use pretty_assertions::assert_eq;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Tag {
        A,
        B,
    }

    fn collect_chunks<P>(parser: &mut P, chunks: &[&str]) -> TextProjection<P::Event>
    where
        P: TextProjector,
    {
        let mut all = TextProjection::default();
        for chunk in chunks {
            all.merge(parser.project(chunk));
        }
        all.merge(parser.close());
        all
    }

    #[test]
    fn generic_inline_parser_supports_multiple_tag_types() {
        let mut parser = HiddenTagProjector::new(vec![
            InlineTagSpec {
                tag: Tag::A,
                open: "<a>",
                close: "</a>",
            },
            InlineTagSpec {
                tag: Tag::B,
                open: "<b>",
                close: "</b>",
            },
        ]);

        let out = collect_chunks(&mut parser, &["1<a>x</a>2<b>y</b>3"]);

        assert_eq!(out.renderable, "123");
        assert_eq!(out.events.len(), 2);
        assert_eq!(out.events[0].tag, Tag::A);
        assert_eq!(out.events[0].content, "x");
        assert_eq!(out.events[1].tag, Tag::B);
        assert_eq!(out.events[1].content, "y");
    }

    #[test]
    fn generic_inline_parser_supports_non_ascii_tag_delimiters() {
        let mut parser = HiddenTagProjector::new(vec![InlineTagSpec {
            tag: Tag::A,
            open: "<é>",
            close: "</é>",
        }]);

        let out = collect_chunks(&mut parser, &["a<", "é>中</", "é>b"]);

        assert_eq!(out.renderable, "ab");
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].tag, Tag::A);
        assert_eq!(out.events[0].content, "中");
    }

    #[test]
    fn generic_inline_parser_prefers_longest_opener_at_same_offset() {
        let mut parser = HiddenTagProjector::new(vec![
            InlineTagSpec {
                tag: Tag::A,
                open: "<a>",
                close: "</a>",
            },
            InlineTagSpec {
                tag: Tag::B,
                open: "<ab>",
                close: "</ab>",
            },
        ]);

        let out = collect_chunks(&mut parser, &["x<ab>y</ab>z"]);

        assert_eq!(out.renderable, "xz");
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].tag, Tag::B);
        assert_eq!(out.events[0].content, "y");
    }

    #[test]
    #[should_panic(expected = "non-empty open delimiters")]
    fn generic_inline_parser_rejects_empty_open_delimiter() {
        let _ = HiddenTagProjector::new(vec![InlineTagSpec {
            tag: Tag::A,
            open: "",
            close: "</a>",
        }]);
    }

    #[test]
    #[should_panic(expected = "non-empty close delimiters")]
    fn generic_inline_parser_rejects_empty_close_delimiter() {
        let _ = HiddenTagProjector::new(vec![InlineTagSpec {
            tag: Tag::A,
            open: "<a>",
            close: "",
        }]);
    }
}
