use super::*;

#[test]
fn approval_required_when_read_only_false_and_destructive() {
    let annotations = annotations(Some(false), Some(true), /*open_world*/ None);
    assert_eq!(requires_mcp_tool_approval(Some(&annotations)), true);
}

#[test]
fn approval_required_when_read_only_false_and_open_world() {
    let annotations = annotations(Some(false), /*destructive*/ None, Some(true));
    assert_eq!(requires_mcp_tool_approval(Some(&annotations)), true);
}

#[test]
fn approval_required_when_destructive_even_if_read_only_true() {
    let annotations = annotations(Some(true), Some(true), Some(true));
    assert_eq!(requires_mcp_tool_approval(Some(&annotations)), true);
}

#[test]
fn approval_required_when_annotations_are_absent() {
    assert_eq!(requires_mcp_tool_approval(/*annotations*/ None), true);
}

#[test]
fn approval_not_required_when_read_only_and_other_hints_are_absent() {
    let annotations = annotations(
        Some(true),
        /*destructive*/ None,
        /*open_world*/ None,
    );
    assert_eq!(requires_mcp_tool_approval(Some(&annotations)), false);
}

#[test]
fn prompt_mode_does_not_allow_persistent_remember() {
    assert_eq!(
        normalize_approval_decision_for_mode(
            McpToolApprovalDecision::AcceptForSession,
            AppToolApproval::Prompt,
        ),
        McpToolApprovalDecision::Accept
    );
    assert_eq!(
        normalize_approval_decision_for_mode(
            McpToolApprovalDecision::AcceptAndRemember,
            AppToolApproval::Prompt,
        ),
        McpToolApprovalDecision::Accept
    );
}

#[test]
fn approval_question_text_prepends_safety_reason() {
    assert_eq!(
        mcp_tool_approval_question_text(
            "Allow this action?".to_string(),
            Some("This tool may contact an external system."),
        ),
        "Tool call needs your approval. Reason: This tool may contact an external system."
    );
}
