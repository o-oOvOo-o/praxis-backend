#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryExecutionPolicy {
    ModelRuntime,
}

pub(crate) const SUMMARY_MODEL_INSTRUCTIONS: &str = "Generate a compact session summary for a conversation picker. Mention the main user goal, the most important progress or result, and the next unresolved step if there is one. Output plain text only, 1-3 sentences, maximum 220 characters.";

pub(crate) fn select_summary_policy() -> SummaryExecutionPolicy {
    SummaryExecutionPolicy::ModelRuntime
}
