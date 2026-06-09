pub(crate) mod behavior;
pub(crate) mod prompts;
pub(crate) mod provider;
pub(crate) mod tasks;
pub(crate) mod tools;

use super::plugin::ProfileAutoTitlePolicyDescriptor;
use super::plugin::ProfileDescriptor;
use super::plugin::ProfilePromptLayerDescriptor;
use super::plugin::ProfileProviderPolicy;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::compact::CompactExecutionPolicy;
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
        provider_policy: Some(ProfileProviderPolicy {
            canonical_provider_id: Some(provider::DEEPSEEK_PROVIDER_ID),
            owner_label: "DeepSeek",
            provider_matches: provider::is_first_party_provider,
            model_matches: provider::is_first_party_model,
        }),
        task_policy: ProfileTaskPolicyDescriptor {
            auto_title: Some(
                ProfileAutoTitlePolicyDescriptor::fixed_without_default_reasoning(
                    DEEPSEEK_AUTO_TITLE_MODEL,
                    AutoTitleProfile::DeepSeekFlash,
                ),
            ),
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
            compact_model: Some(DEEPSEEK_AUTO_TITLE_MODEL),
            auto_compact_token_limit_cap: Some(96_000),
        },
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 1000,
    }
}
