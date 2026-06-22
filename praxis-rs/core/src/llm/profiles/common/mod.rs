mod prompts;
mod provider;

use super::plugin::ProfileDescriptor;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::title::AutoTitleProfile;

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::Common,
        label: "OpenAI-compatible",
        instructions: Some(prompts::BASE),
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: None,
        task_policy: ProfileTaskPolicyDescriptor::local_prompt_with_current_title(
            AutoTitleProfile::Common,
        ),
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 100,
    }
}

pub(crate) fn is_generic_provider(provider_id: &str, provider: &crate::ModelProviderInfo) -> bool {
    provider::is_generic_provider(provider_id, provider)
}

pub(crate) fn is_generic_model(model: &str) -> bool {
    provider::is_generic_model(model)
}
