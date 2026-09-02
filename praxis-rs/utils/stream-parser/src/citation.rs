use crate::HiddenTagProjector;
use crate::InlineTagSpec;
use crate::TextProjection;
use crate::TextProjector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CitationTag {
    Citation,
}

const CITATION_OPEN: &str = "<praxis-memory-citation>";
const CITATION_CLOSE: &str = "</praxis-memory-citation>";

/// Stream parser for `<praxis-memory-citation>...</praxis-memory-citation>` tags.
///
/// This is a thin convenience wrapper around [`HiddenTagProjector`]. It returns citation bodies
/// as plain strings and omits the citation tags from visible text.
///
/// Matching is literal and non-nested. If EOF is reached before a closing
/// `</praxis-memory-citation>`, the parser auto-closes the tag and returns the buffered body as an
/// extracted citation.
#[derive(Debug)]
pub struct CitationProjector {
    inner: HiddenTagProjector<CitationTag>,
}

impl CitationProjector {
    pub fn new() -> Self {
        Self {
            inner: HiddenTagProjector::new(vec![InlineTagSpec {
                tag: CitationTag::Citation,
                open: CITATION_OPEN,
                close: CITATION_CLOSE,
            }]),
        }
    }
}

impl Default for CitationProjector {
    fn default() -> Self {
        Self::new()
    }
}

impl TextProjector for CitationProjector {
    type Event = String;

    fn project(&mut self, input: &str) -> TextProjection<Self::Event> {
        citation_projection(self.inner.project(input))
    }

    fn close(&mut self) -> TextProjection<Self::Event> {
        citation_projection(self.inner.close())
    }
}

fn citation_projection(
    projection: TextProjection<crate::ExtractedInlineTag<CitationTag>>,
) -> TextProjection<String> {
    projection.map_events(|tag| tag.content)
}

/// Strip citation tags from a complete string and return `(visible_text, citations)`.
///
/// This uses [`CitationProjector`] internally, so it inherits the same semantics:
/// literal, non-nested matching and auto-closing unterminated citations at EOF.
pub fn strip_citations(text: &str) -> (String, Vec<String>) {
    let mut parser = CitationProjector::new();
    let mut out = parser.project(text);
    out.merge(parser.close());
    (out.renderable, out.events)
}

#[cfg(test)]
mod tests {
    use super::CitationProjector;
    use super::strip_citations;
    use crate::TextProjection;
    use crate::TextProjector;
    use pretty_assertions::assert_eq;

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
    fn citation_parser_streams_across_chunk_boundaries() {
        let mut parser = CitationProjector::new();
        let out = collect_chunks(
            &mut parser,
            &[
                "Hello <praxis-memory-",
                "citation>source A</praxis-memory-",
                "citation> world",
            ],
        );

        assert_eq!(out.renderable, "Hello  world");
        assert_eq!(out.events, vec!["source A".to_string()]);
    }

    #[test]
    fn citation_parser_buffers_partial_open_tag_prefix() {
        let mut parser = CitationProjector::new();

        let first = parser.project("abc <praxis-memory-");
        assert_eq!(first.renderable, "abc ");
        assert_eq!(first.events, Vec::<String>::new());

        let second = parser.project("citation>x</praxis-memory-citation>z");
        let tail = parser.close();

        assert_eq!(second.renderable, "z");
        assert_eq!(second.events, vec!["x".to_string()]);
        assert!(tail.is_empty());
    }

    #[test]
    fn citation_parser_auto_closes_unterminated_tag_on_finish() {
        let mut parser = CitationProjector::new();
        let out = collect_chunks(&mut parser, &["x<praxis-memory-citation>source"]);

        assert_eq!(out.renderable, "x");
        assert_eq!(out.events, vec!["source".to_string()]);
    }

    #[test]
    fn citation_parser_preserves_partial_open_tag_at_eof_if_not_a_full_tag() {
        let mut parser = CitationProjector::new();
        let out = collect_chunks(&mut parser, &["hello <praxis-memory-"]);

        assert_eq!(out.renderable, "hello <praxis-memory-");
        assert_eq!(out.events, Vec::<String>::new());
    }

    #[test]
    fn strip_citations_collects_all_citations() {
        let (visible, citations) = strip_citations(
            "a<praxis-memory-citation>one</praxis-memory-citation>b<praxis-memory-citation>two</praxis-memory-citation>c",
        );

        assert_eq!(visible, "abc");
        assert_eq!(citations, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn strip_citations_auto_closes_unterminated_citation_at_eof() {
        let (visible, citations) = strip_citations("x<praxis-memory-citation>y");

        assert_eq!(visible, "x");
        assert_eq!(citations, vec!["y".to_string()]);
    }

    #[test]
    fn citation_parser_does_not_support_nested_tags() {
        let (visible, citations) = strip_citations(
            "a<praxis-memory-citation>x<praxis-memory-citation>y</praxis-memory-citation>z</praxis-memory-citation>b",
        );

        assert_eq!(visible, "az</praxis-memory-citation>b");
        assert_eq!(citations, vec!["x<praxis-memory-citation>y".to_string()]);
    }
}
