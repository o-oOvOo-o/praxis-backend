use crate::llm::prompts::LlmPromptPurpose;
use crate::praxis::AutoSummaryModelContext;
use crate::praxis::Session;

use super::base::SessionModelRuntimeContext;

impl Session {
    /// Returns the current model runtime context for automatic thread summaries.
    pub(crate) async fn auto_summary_model_context(&self) -> AutoSummaryModelContext {
        let base = SessionModelRuntimeContext::capture(self).await;
        let instructions = base.resolve_prompt(self, LlmPromptPurpose::AutoSummary);
        AutoSummaryModelContext {
            provider_id: base.per_turn_config.model_provider_id.clone(),
            provider: base.per_turn_config.model_provider.clone(),
            model_info: base.current_model_info.clone(),
            instructions,
            session_telemetry: self.services.session_telemetry.clone().with_model(
                base.current_model_slug.as_str(),
                base.current_model_info.slug.as_str(),
            ),
            service_tier: base.session_configuration.service_tier,
            personality: base.session_configuration.personality,
        }
    }
}
