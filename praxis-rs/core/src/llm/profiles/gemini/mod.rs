pub(crate) mod behavior;
pub(crate) mod prompts;
pub(crate) mod provider;
pub(crate) mod tasks;
pub(crate) mod tools;

use super::plugin::ProfileAutoTitlePolicyDescriptor;
use super::plugin::ProfileDescriptor;
use super::plugin::ProfileProviderPolicy;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::compact::CompactExecutionPolicy;
use crate::llm::tasks::title::AutoTitleProfile;

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::Gemini,
        label: "Gemini",
        instructions: Some(prompts::BASE),
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy {
            canonical_provider_id: Some(provider::GEMINI_PROVIDER_ID),
            owner_label: "Gemini",
            provider_matches: provider::is_first_party_provider,
            model_matches: provider::is_first_party_model,
        }),
        task_policy: ProfileTaskPolicyDescriptor {
            auto_title: Some(ProfileAutoTitlePolicyDescriptor::current(
                AutoTitleProfile::Common,
            )),
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
            compact_model: None,
            auto_compact_token_limit_cap: None,
        },
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 875,
    }
}
