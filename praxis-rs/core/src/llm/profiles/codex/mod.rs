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
        id: BehaviorProfileId::CodexResponses,
        label: "Codex Responses",
        instructions: Some(prompts::RESPONSES),
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: Some(ProfileProviderPolicy {
            canonical_provider_id: Some(crate::model_provider_info::OPENAI_PROVIDER_ID),
            owner_label: "OpenAI/Codex",
            provider_matches: provider::is_first_party_provider,
            model_matches: provider::is_first_party_model,
        }),
        task_policy: ProfileTaskPolicyDescriptor {
            auto_title: Some(ProfileAutoTitlePolicyDescriptor::current(
                AutoTitleProfile::CodexResponses,
            )),
            compact_execution: Some(CompactExecutionPolicy::RemoteResponses),
            compact_model: None,
            auto_compact_token_limit_cap: None,
        },
        tool_capabilities: ProfileToolCapabilityDescriptor::responses_web_search(),
        priority: 700,
    }
}
