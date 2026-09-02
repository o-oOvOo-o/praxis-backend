use crate::CitationProjector;
use crate::ProposedPlanProjector;
use crate::ProposedPlanSegment;
use crate::TextProjection;
use crate::TextProjector;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssistantTextChunk {
    pub visible_text: String,
    pub citations: Vec<String>,
    pub plan_segments: Vec<ProposedPlanSegment>,
}

impl AssistantTextChunk {
    pub fn is_empty(&self) -> bool {
        self.visible_text.is_empty() && self.citations.is_empty() && self.plan_segments.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssistantTextFeatures {
    pub citations: bool,
    pub proposed_plan: bool,
}

impl Default for AssistantTextFeatures {
    fn default() -> Self {
        Self {
            citations: true,
            proposed_plan: false,
        }
    }
}

/// Parses assistant text streaming markup in one pass:
/// - strips `<praxis-memory-citation>` tags and extracts citation payloads
/// - in plan mode, also strips `<proposed_plan>` blocks and emits plan segments
#[derive(Debug)]
pub struct AssistantTextProjector {
    citations: Option<CitationProjector>,
    plan: Option<ProposedPlanProjector>,
}

impl AssistantTextProjector {
    pub fn new(plan_mode: bool) -> Self {
        Self::with_features(AssistantTextFeatures {
            proposed_plan: plan_mode,
            ..AssistantTextFeatures::default()
        })
    }

    pub fn with_features(features: AssistantTextFeatures) -> Self {
        Self {
            citations: features.citations.then(CitationProjector::default),
            plan: features.proposed_plan.then(ProposedPlanProjector::default),
        }
    }

    pub fn project(&mut self, input: &str) -> AssistantTextChunk {
        let citations = self.citations.as_mut().map_or_else(
            || TextProjection::renderable(input),
            |projector| projector.project(input),
        );
        self.route(citations)
    }

    pub fn close(&mut self) -> AssistantTextChunk {
        let citation_tail = self
            .citations
            .as_mut()
            .map(TextProjector::close)
            .unwrap_or_default();
        let mut out = self.route(citation_tail);
        if let Some(plan) = &mut self.plan {
            let tail = plan.close();
            out.visible_text.push_str(&tail.renderable);
            out.plan_segments.extend(tail.events);
        }
        out
    }

    fn route(&mut self, citations: TextProjection<String>) -> AssistantTextChunk {
        let Some(plan) = &mut self.plan else {
            return AssistantTextChunk {
                visible_text: citations.renderable,
                citations: citations.events,
                ..AssistantTextChunk::default()
            };
        };
        let plan_projection = plan.project(&citations.renderable);
        AssistantTextChunk {
            visible_text: plan_projection.renderable,
            citations: citations.events,
            plan_segments: plan_projection.events,
        }
    }
}

impl Default for AssistantTextProjector {
    fn default() -> Self {
        Self::with_features(AssistantTextFeatures::default())
    }
}

#[cfg(test)]
mod tests {
    use super::AssistantTextProjector;
    use crate::ProposedPlanSegment;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_citations_across_seed_and_delta_boundaries() {
        let mut parser = AssistantTextProjector::new(/*plan_mode*/ false);

        let seeded = parser.project("hello <praxis-memory-citation>doc");
        let parsed = parser.project("1</praxis-memory-citation> world");
        let tail = parser.close();

        assert_eq!(seeded.visible_text, "hello ");
        assert_eq!(seeded.citations, Vec::<String>::new());
        assert_eq!(parsed.visible_text, " world");
        assert_eq!(parsed.citations, vec!["doc1".to_string()]);
        assert_eq!(tail.visible_text, "");
        assert_eq!(tail.citations, Vec::<String>::new());
    }

    #[test]
    fn parses_plan_segments_after_citation_stripping() {
        let mut parser = AssistantTextProjector::new(/*plan_mode*/ true);

        let seeded = parser.project("Intro\n<proposed");
        let parsed =
            parser.project("_plan>\n- step <praxis-memory-citation>doc</praxis-memory-citation>\n");
        let tail = parser.project("</proposed_plan>\nOutro");
        let finish = parser.close();

        assert_eq!(seeded.visible_text, "Intro\n");
        assert_eq!(
            seeded.plan_segments,
            vec![ProposedPlanSegment::Normal("Intro\n".to_string())]
        );
        assert_eq!(parsed.visible_text, "");
        assert_eq!(parsed.citations, vec!["doc".to_string()]);
        assert_eq!(
            parsed.plan_segments,
            vec![
                ProposedPlanSegment::ProposedPlanStart,
                ProposedPlanSegment::ProposedPlanDelta("- step \n".to_string()),
            ]
        );
        assert_eq!(tail.visible_text, "Outro");
        assert_eq!(
            tail.plan_segments,
            vec![
                ProposedPlanSegment::ProposedPlanEnd,
                ProposedPlanSegment::Normal("Outro".to_string()),
            ]
        );
        assert!(finish.is_empty());
    }
}
