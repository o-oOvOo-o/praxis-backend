mod provider;

use super::deepseek;
use super::plugin::ProfileDescriptor;
use super::plugin::ProfilePromptLayerDescriptor;
use super::plugin::ProfileProviderPolicy;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::title::AutoTitleProfile;

const PROMPT_LAYERS: &[ProfilePromptLayerDescriptor] =
    &[ProfilePromptLayerDescriptor::model_instructions(
        "kimi/smarter",
        deepseek::SMARTER_INSTRUCTIONS,
    )];

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::Kimi,
        #[cfg(test)]
        label: "Kimi",
        instructions: Some(deepseek::BASE_INSTRUCTIONS),
        prompt_layers: PROMPT_LAYERS,
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy::first_party(
            crate::llm::provider::KIMI_PROVIDER_ID,
            "Kimi",
            is_first_party_provider,
            is_first_party_model,
        )),
        task_policy: ProfileTaskPolicyDescriptor::local_prompt_with_current_title(
            AutoTitleProfile::Common,
        ),
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 950,
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
