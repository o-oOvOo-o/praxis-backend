mod prompts;
mod provider;

use super::plugin::ProfileDescriptor;
use super::plugin::ProfilePromptLayerDescriptor;
use super::plugin::ProfileProviderPolicy;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;

const PROMPT_LAYERS: &[ProfilePromptLayerDescriptor] =
    &[ProfilePromptLayerDescriptor::model_instructions_for(
        "claude/fable-5",
        prompts::FABLE_5,
        provider::is_fable_5,
    )];

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::Claude,
        #[cfg(test)]
        label: "Claude",
        instructions: Some(prompts::BASE),
        prompt_layers: PROMPT_LAYERS,
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy::first_party(
            crate::model_provider_info::ANTHROPIC_PROVIDER_ID,
            "Anthropic",
            is_first_party_provider,
            is_first_party_model,
        )),
        task_policy: ProfileTaskPolicyDescriptor::local_prompt(),
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 600,
    }
}

pub(crate) fn is_first_party_provider(
    provider_id: &str,
    provider: &crate::model_provider_info::ModelProviderInfo,
) -> bool {
    provider::is_first_party_provider(provider_id, provider)
}

pub(crate) fn is_first_party_model(model: &str) -> bool {
    provider::is_first_party_model(model)
}
