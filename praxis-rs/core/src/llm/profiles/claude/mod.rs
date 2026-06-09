pub(crate) mod behavior;
pub(crate) mod prompts;
pub(crate) mod provider;
pub(crate) mod tasks;
pub(crate) mod tools;

use super::plugin::ProfileDescriptor;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::compact::CompactExecutionPolicy;

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::Claude,
        label: "Claude",
        instructions: None,
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: None,
        task_policy: ProfileTaskPolicyDescriptor {
            auto_title: None,
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
            compact_model: None,
            auto_compact_token_limit_cap: None,
        },
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 600,
    }
}
