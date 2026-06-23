use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;

use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;

pub(super) fn resolve_base_instructions(
    config: &Config,
    conversation_history: &InitialHistory,
    model_info: &ModelInfo,
    session_source: &SessionSource,
    llm_runtime_catalog: &LlmRuntimeCatalog,
) -> String {
    config
        .base_instructions
        .clone()
        .or_else(|| conversation_history.get_base_instructions().map(|s| s.text))
        .unwrap_or_else(|| {
            crate::prompt_profiles::resolve_model_instructions(
                model_info,
                &config.model_provider_id,
                &config.model_provider,
                config.personality,
                session_source
                    .restriction_product()
                    .and_then(crate::llm::ids::ProductProfileId::from_product),
                llm_runtime_catalog,
            )
        })
}
