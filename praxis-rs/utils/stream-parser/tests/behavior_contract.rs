use praxis_utils_stream_parser::AssistantTextChunk;
use praxis_utils_stream_parser::AssistantTextProjector;
use praxis_utils_stream_parser::CitationProjector;
use praxis_utils_stream_parser::ExtractedInlineTag;
use praxis_utils_stream_parser::HiddenTagProjector;
use praxis_utils_stream_parser::InlineTagSpec;
use praxis_utils_stream_parser::ProposedPlanProjector;
use praxis_utils_stream_parser::ProposedPlanSegment;
use praxis_utils_stream_parser::TextProjection;
use praxis_utils_stream_parser::TextProjector;
use praxis_utils_stream_parser::Utf8ProjectionAdapter;

fn collect<P: TextProjector>(mut parser: P, chunks: &[&str]) -> TextProjection<P::Event> {
    let mut out = TextProjection::default();
    for chunk in chunks {
        out.merge(parser.project(chunk));
    }
    out.merge(parser.close());
    out
}

fn collect_assistant(chunks: &[&str]) -> AssistantTextChunk {
    let mut parser = AssistantTextProjector::new(true);
    let mut out = AssistantTextChunk::default();
    for chunk in chunks {
        let mut next = parser.project(chunk);
        out.visible_text.push_str(&next.visible_text);
        out.citations.append(&mut next.citations);
        out.plan_segments.append(&mut next.plan_segments);
    }
    let mut tail = parser.close();
    out.visible_text.push_str(&tail.visible_text);
    out.citations.append(&mut tail.citations);
    out.plan_segments.append(&mut tail.plan_segments);
    out.plan_segments = normalize_plan_segments(out.plan_segments);
    out
}

fn normalize_plan_segments(segments: Vec<ProposedPlanSegment>) -> Vec<ProposedPlanSegment> {
    let mut out = Vec::new();
    for segment in segments {
        match segment {
            ProposedPlanSegment::Normal(text) => match out.last_mut() {
                Some(ProposedPlanSegment::Normal(existing)) => existing.push_str(&text),
                _ => out.push(ProposedPlanSegment::Normal(text)),
            },
            ProposedPlanSegment::ProposedPlanDelta(text) => match out.last_mut() {
                Some(ProposedPlanSegment::ProposedPlanDelta(existing)) => existing.push_str(&text),
                _ => out.push(ProposedPlanSegment::ProposedPlanDelta(text)),
            },
            boundary => out.push(boundary),
        }
    }
    out
}

fn char_splits(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .map(|index| text.split_at(index))
}

#[test]
fn citation_result_is_independent_of_character_chunk_boundaries() {
    let input = "α <praxis-memory-citation>文档 A</praxis-memory-citation> β \
        <praxis-memory-citation>unfinished";
    let expected = collect(CitationProjector::new(), &[input]);
    for (left, right) in char_splits(input) {
        assert_eq!(collect(CitationProjector::new(), &[left, right]), expected);
    }
}

#[test]
fn multiple_inline_tags_keep_declaration_order_and_longest_open_match() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Tag {
        Short,
        Long,
    }
    let parser = || {
        HiddenTagProjector::new(vec![
            InlineTagSpec {
                tag: Tag::Short,
                open: "<x>",
                close: "</x>",
            },
            InlineTagSpec {
                tag: Tag::Long,
                open: "<x><y>",
                close: "</y></x>",
            },
        ])
    };
    let input = "a<x><y>long</y></x>b<x>short</x>c";
    let expected = TextProjection {
        renderable: "abc".to_string(),
        events: vec![
            ExtractedInlineTag {
                tag: Tag::Long,
                content: "long".to_string(),
            },
            ExtractedInlineTag {
                tag: Tag::Short,
                content: "short".to_string(),
            },
        ],
    };
    assert_eq!(collect(parser(), &[input]), expected);
    for (left, right) in char_splits(input) {
        assert_eq!(collect(parser(), &[left, right]), expected);
    }
}

#[test]
fn assistant_pipeline_preserves_plan_and_citation_semantics_for_every_split() {
    let input = "Intro\n<proposed_plan>\n- inspect \
        <praxis-memory-citation>memory-1</praxis-memory-citation>code\n</proposed_plan>\nOutro \
        <praxis-memory-citation>memory-2";
    let expected = collect_assistant(&[input]);
    assert_eq!(expected.visible_text, "Intro\nOutro ");
    assert_eq!(expected.citations, ["memory-1", "memory-2"]);
    assert_eq!(
        expected.plan_segments,
        [
            ProposedPlanSegment::Normal("Intro\n".to_string()),
            ProposedPlanSegment::ProposedPlanStart,
            ProposedPlanSegment::ProposedPlanDelta("- inspect code\n".to_string()),
            ProposedPlanSegment::ProposedPlanEnd,
            ProposedPlanSegment::Normal("Outro ".to_string()),
        ]
    );
    for (left, right) in char_splits(input) {
        assert_eq!(collect_assistant(&[left, right]), expected);
    }
}

#[test]
fn proposed_plan_accepts_indented_crlf_markers_without_exposing_them() {
    let input = "before\r\n  <proposed_plan>  \r\nstep\r\n\t</proposed_plan>\r\nafter";
    let mut actual = collect(ProposedPlanProjector::new(), &[input]);
    actual.events = normalize_plan_segments(actual.events);
    assert_eq!(actual.renderable, "before\r\nafter");
    assert_eq!(
        actual.events,
        [
            ProposedPlanSegment::Normal("before\r\n".to_string()),
            ProposedPlanSegment::ProposedPlanStart,
            ProposedPlanSegment::ProposedPlanDelta("step\r\n".to_string()),
            ProposedPlanSegment::ProposedPlanEnd,
            ProposedPlanSegment::Normal("after".to_string()),
        ]
    );
}

#[test]
fn utf8_adapter_preserves_text_for_every_byte_split() {
    let input = "前缀 <praxis-memory-citation>来源</praxis-memory-citation> 后缀";
    let expected = collect(CitationProjector::new(), &[input]);
    for split in 0..=input.len() {
        let mut parser = Utf8ProjectionAdapter::new(CitationProjector::new());
        let mut actual = TextProjection::default();
        for bytes in [&input.as_bytes()[..split], &input.as_bytes()[split..]] {
            let next = parser.push_bytes(bytes).expect("valid split UTF-8 stream");
            actual.merge(next);
        }
        actual.merge(parser.close().expect("valid UTF-8 finish"));
        assert_eq!(actual, expected, "byte split {split}");
    }
}

#[test]
fn finish_is_idempotent_after_all_pending_state_is_drained() {
    let mut citations = CitationProjector::new();
    citations.project("visible<praxis-memory-citation>hidden");
    assert_eq!(citations.close().events, ["hidden"]);
    assert!(citations.close().is_empty());

    let mut plan = ProposedPlanProjector::new();
    let projected = plan.project("<proposed_plan>\nunfinished");
    assert_eq!(
        projected.events,
        [
            ProposedPlanSegment::ProposedPlanStart,
            ProposedPlanSegment::ProposedPlanDelta("unfinished".to_string()),
        ]
    );
    assert_eq!(plan.close().events, [ProposedPlanSegment::ProposedPlanEnd]);
    assert!(plan.close().is_empty());
}

#[test]
fn utf8_finish_does_not_invent_an_empty_push_for_the_wrapped_parser() {
    #[derive(Default)]
    struct PushObserver;

    impl TextProjector for PushObserver {
        type Event = ();

        fn project(&mut self, input: &str) -> TextProjection<Self::Event> {
            TextProjection {
                renderable: format!("project:{input}"),
                events: Vec::new(),
            }
        }

        fn close(&mut self) -> TextProjection<Self::Event> {
            TextProjection {
                renderable: "close".to_string(),
                events: Vec::new(),
            }
        }
    }

    let mut parser = Utf8ProjectionAdapter::new(PushObserver);
    assert_eq!(parser.close().unwrap().renderable, "close");
}
