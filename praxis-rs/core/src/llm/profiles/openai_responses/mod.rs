mod prompts;
mod provider;

use super::plugin::ProfileDescriptor;
use super::plugin::ProfileProviderPolicy;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::title::AutoTitleProfile;

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::OpenAiResponses,
        #[cfg(test)]
        label: "OpenAI Responses",
        instructions: Some(prompts::RESPONSES),
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy::first_party(
            crate::model_provider_info::OPENAI_PROVIDER_ID,
            "OpenAI Responses",
            is_first_party_provider,
            is_first_party_model,
        )),
        task_policy: ProfileTaskPolicyDescriptor::remote_responses_with_current_title(
            AutoTitleProfile::OpenAiResponses,
        ),
        tool_capabilities: ProfileToolCapabilityDescriptor::responses_web_search(),
        priority: 700,
    }
}

pub(crate) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::ModelProviderInfo,
) -> bool {
    provider::is_first_party_provider(provider_id, provider)
}

pub(crate) fn is_first_party_model(model: &str) -> bool {
    provider::is_first_party_model(model)
}
