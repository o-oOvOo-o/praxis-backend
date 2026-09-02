use crate::TextProjection;
use crate::TextProjector;
use crate::tagged_line_parser::TagSpec;
use crate::tagged_line_parser::TaggedLineParser;
use crate::tagged_line_parser::TaggedLineSegment;

const OPEN_TAG: &str = "<proposed_plan>";
const CLOSE_TAG: &str = "</proposed_plan>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanTag {
    ProposedPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposedPlanSegment {
    Normal(String),
    ProposedPlanStart,
    ProposedPlanDelta(String),
    ProposedPlanEnd,
}

/// Parser for `<proposed_plan>` blocks emitted in plan mode.
///
/// Implements [`TextProjector`] so hosts receive renderable text separately
/// from ordered plan events.
#[derive(Debug)]
pub struct ProposedPlanProjector {
    parser: TaggedLineParser<PlanTag>,
}

impl ProposedPlanProjector {
    pub fn new() -> Self {
        Self {
            parser: TaggedLineParser::new(vec![TagSpec {
                open: OPEN_TAG,
                close: CLOSE_TAG,
                tag: PlanTag::ProposedPlan,
            }]),
        }
    }
}

impl Default for ProposedPlanProjector {
    fn default() -> Self {
        Self::new()
    }
}

impl TextProjector for ProposedPlanProjector {
    type Event = ProposedPlanSegment;

    fn project(&mut self, input: &str) -> TextProjection<Self::Event> {
        map_segments(self.parser.parse(input))
    }

    fn close(&mut self) -> TextProjection<Self::Event> {
        map_segments(self.parser.finish())
    }
}

fn map_segments(segments: Vec<TaggedLineSegment<PlanTag>>) -> TextProjection<ProposedPlanSegment> {
    let mut out = TextProjection::default();
    for segment in segments {
        match segment {
            TaggedLineSegment::Normal(text) => {
                out.renderable.push_str(&text);
                out.events.push(ProposedPlanSegment::Normal(text));
            }
            TaggedLineSegment::TagStart(PlanTag::ProposedPlan) => {
                out.events.push(ProposedPlanSegment::ProposedPlanStart);
            }
            TaggedLineSegment::TagDelta(PlanTag::ProposedPlan, text) => {
                out.events
                    .push(ProposedPlanSegment::ProposedPlanDelta(text));
            }
            TaggedLineSegment::TagEnd(PlanTag::ProposedPlan) => {
                out.events.push(ProposedPlanSegment::ProposedPlanEnd);
            }
        }
    }
    out
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProposedPlanCapture {
    saw_plan: bool,
    active: bool,
    text: String,
}

impl ProposedPlanCapture {
    pub fn observe(&mut self, segment: &ProposedPlanSegment) {
        match segment {
            ProposedPlanSegment::ProposedPlanStart => {
                self.saw_plan = true;
                self.active = true;
                self.text.clear();
            }
            ProposedPlanSegment::ProposedPlanDelta(delta) if self.active => {
                self.text.push_str(delta);
            }
            ProposedPlanSegment::ProposedPlanEnd => self.active = false,
            ProposedPlanSegment::Normal(_) | ProposedPlanSegment::ProposedPlanDelta(_) => {}
        }
    }

    pub fn captured(&self) -> Option<&str> {
        self.saw_plan.then_some(self.text.as_str())
    }
}

pub fn strip_proposed_plan_blocks(text: &str) -> String {
    let mut parser = ProposedPlanProjector::new();
    let mut out = parser.project(text);
    out.merge(parser.close());
    out.renderable
}

pub fn extract_proposed_plan_text(text: &str) -> Option<String> {
    let mut parser = ProposedPlanProjector::new();
    let mut capture = ProposedPlanCapture::default();
    for segment in parser
        .project(text)
        .events
        .into_iter()
        .chain(parser.close().events)
    {
        capture.observe(&segment);
    }
    capture.captured().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::ProposedPlanProjector;
    use super::ProposedPlanSegment;
    use super::extract_proposed_plan_text;
    use super::strip_proposed_plan_blocks;
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
    fn streams_proposed_plan_segments_and_visible_text() {
        let mut parser = ProposedPlanProjector::new();
        let out = collect_chunks(
            &mut parser,
            &[
                "Intro text\n<prop",
                "osed_plan>\n- step 1\n",
                "</proposed_plan>\nOutro",
            ],
        );

        assert_eq!(out.renderable, "Intro text\nOutro");
        assert_eq!(
            out.events,
            vec![
                ProposedPlanSegment::Normal("Intro text\n".to_string()),
                ProposedPlanSegment::ProposedPlanStart,
                ProposedPlanSegment::ProposedPlanDelta("- step 1\n".to_string()),
                ProposedPlanSegment::ProposedPlanEnd,
                ProposedPlanSegment::Normal("Outro".to_string()),
            ]
        );
    }

    #[test]
    fn preserves_non_tag_lines() {
        let mut parser = ProposedPlanProjector::new();
        let out = collect_chunks(&mut parser, &["  <proposed_plan> extra\n"]);

        assert_eq!(out.renderable, "  <proposed_plan> extra\n");
        assert_eq!(
            out.events,
            vec![ProposedPlanSegment::Normal(
                "  <proposed_plan> extra\n".to_string()
            )]
        );
    }

    #[test]
    fn closes_unterminated_plan_block_on_finish() {
        let mut parser = ProposedPlanProjector::new();
        let out = collect_chunks(&mut parser, &["<proposed_plan>\n- step 1\n"]);

        assert_eq!(out.renderable, "");
        assert_eq!(
            out.events,
            vec![
                ProposedPlanSegment::ProposedPlanStart,
                ProposedPlanSegment::ProposedPlanDelta("- step 1\n".to_string()),
                ProposedPlanSegment::ProposedPlanEnd,
            ]
        );
    }

    #[test]
    fn strips_proposed_plan_blocks_from_text() {
        let text = "before\n<proposed_plan>\n- step\n</proposed_plan>\nafter";
        assert_eq!(strip_proposed_plan_blocks(text), "before\nafter");
    }

    #[test]
    fn extracts_proposed_plan_text() {
        let text = "before\n<proposed_plan>\n- step\n</proposed_plan>\nafter";
        assert_eq!(
            extract_proposed_plan_text(text),
            Some("- step\n".to_string())
        );
    }
}
