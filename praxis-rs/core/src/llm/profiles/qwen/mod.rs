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
        id: BehaviorProfileId::Qwen,
        #[cfg(test)]
        label: "Qwen",
        instructions: Some(prompts::BASE),
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy::first_party(
            provider::QWEN_PROVIDER_ID,
            "Qwen",
            is_first_party_provider,
            is_first_party_model,
        )),
        task_policy: ProfileTaskPolicyDescriptor::local_prompt_with_current_title(
            AutoTitleProfile::Common,
        ),
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 900,
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
