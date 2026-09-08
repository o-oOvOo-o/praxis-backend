use praxis_protocol::openai_models::DEFAULT_REASONING_EFFORT;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ReasoningEffort;

pub(super) fn resolve(
    current_reasoning_effort: Option<ReasoningEffort>,
    model_info: &ModelInfo,
) -> Option<ReasoningEffort> {
    Some(
        current_reasoning_effort
            .filter(|effort| model_info.supports_reasoning_effort(effort))
            .unwrap_or(DEFAULT_REASONING_EFFORT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_protocol::openai_models::known_openai_compatible_model_info;

    #[test]
    fn missing_or_unsupported_effort_defaults_to_medium() {
        let model = known_openai_compatible_model_info("gpt-6-astra").unwrap();
        for effort in [None, Some(ReasoningEffort::Minimal)] {
            assert_eq!(resolve(effort, &model), Some(ReasoningEffort::Medium));
        }
        assert_eq!(
            resolve(Some(ReasoningEffort::High), &model),
            Some(ReasoningEffort::High)
        );
    }
}
