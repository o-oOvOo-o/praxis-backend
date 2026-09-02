use praxis_utils_stream_parser::AssistantTextProjector;
use praxis_utils_stream_parser::CitationProjector;
use praxis_utils_stream_parser::ProposedPlanSegment;
use praxis_utils_stream_parser::Utf8ProjectionAdapter;

fn main() -> Result<(), praxis_utils_stream_parser::Utf8ProjectionError> {
    let mut assistant = AssistantTextProjector::new(true);
    let first = assistant.project("Intro\n<proposed_plan>\nstep <praxis-memory-");
    let second =
        assistant.project("citation>source</praxis-memory-citation>\n</proposed_plan>\nDone");
    let tail = assistant.close();
    assert_eq!(
        [first.visible_text, second.visible_text, tail.visible_text].concat(),
        "Intro\nDone"
    );
    assert_eq!(
        [first.citations, second.citations, tail.citations].concat(),
        ["source"]
    );
    assert_eq!(
        [
            first.plan_segments,
            second.plan_segments,
            tail.plan_segments
        ]
        .concat(),
        [
            ProposedPlanSegment::Normal("Intro\n".to_string()),
            ProposedPlanSegment::ProposedPlanStart,
            ProposedPlanSegment::ProposedPlanDelta("step ".to_string()),
            ProposedPlanSegment::ProposedPlanDelta("\n".to_string()),
            ProposedPlanSegment::ProposedPlanEnd,
            ProposedPlanSegment::Normal("Done".to_string()),
        ]
    );

    let mut utf8 = Utf8ProjectionAdapter::new(CitationProjector::new());
    assert_eq!(utf8.push_bytes(&[b'H', 0xC3])?.renderable, "H");
    assert_eq!(utf8.push_bytes(&[0xA9, b'!'])?.renderable, "é!");
    assert!(utf8.close()?.is_empty());
    Ok(())
}
