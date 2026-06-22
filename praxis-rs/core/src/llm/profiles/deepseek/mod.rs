mod prompts;
mod provider;

use super::plugin::ProfileDescriptor;
use super::plugin::ProfilePromptLayerDescriptor;
use super::plugin::ProfileProviderPolicy;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::title::AutoTitleProfile;
use crate::llm::tasks::title::DEEPSEEK_AUTO_TITLE_MODEL;

const PROMPT_LAYERS: &[ProfilePromptLayerDescriptor] =
    &[ProfilePromptLayerDescriptor::model_instructions(
        "deepseek/smarter",
        prompts::SMARTER,
    )];

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::DeepSeek,
        label: "DeepSeek",
        instructions: Some(prompts::BASE),
        prompt_layers: PROMPT_LAYERS,
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy::first_party(
            provider::DEEPSEEK_PROVIDER_ID,
            "DeepSeek",
            is_first_party_provider,
            is_first_party_model,
        )),
        task_policy: ProfileTaskPolicyDescriptor::local_prompt_with_fixed_title_model(
            DEEPSEEK_AUTO_TITLE_MODEL,
            AutoTitleProfile::DeepSeekFlash,
            Some(96_000),
        ),
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 1000,
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
