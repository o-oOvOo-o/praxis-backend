use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort;

pub(super) fn resolve(
    current_reasoning_effort: Option<ReasoningEffort>,
    model_info: &ModelInfo,
) -> Option<ReasoningEffort> {
    let supported_reasoning_levels = model_info
        .supported_reasoning_levels
        .iter()
        .map(|preset| preset.effort)
        .collect::<Vec<_>>();
    if let Some(current_reasoning_effort) = current_reasoning_effort
        && supported_reasoning_levels.contains(&current_reasoning_effort)
    {
        return Some(current_reasoning_effort);
    }
    supported_reasoning_levels
        .get(supported_reasoning_levels.len().saturating_sub(1) / 2)
        .copied()
        .or(model_info.default_reasoning_level)
}
