use super::super::super::TurnContext;

pub(super) fn build_model_spec(turn_context: &TurnContext) -> praxis_loop::model::ModelSpec {
    praxis_loop::model::ModelSpec {
        slug: turn_context.model_info.slug.clone(),
        provider_id: Some(turn_context.config.model_provider_id.clone()),
        context_window: loop_context_window(turn_context.model_context_window()),
        input_modalities: turn_context
            .model_info
            .input_modalities
            .iter()
            .map(|modality| format!("{modality:?}"))
            .collect(),
    }
}

fn loop_context_window(value: Option<i64>) -> Option<u64> {
    let Some(value) = value else {
        return None;
    };
    match u64::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => None,
    }
}
