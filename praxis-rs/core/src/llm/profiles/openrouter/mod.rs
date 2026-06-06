pub(crate) mod behavior;
pub(crate) mod prompts;
pub(crate) mod provider;
pub(crate) mod tasks;
pub(crate) mod tools;

use super::plugin::ProfileAutoTitlePolicyDescriptor;
use super::plugin::ProfileDescriptor;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::tasks::compact::CompactExecutionPolicy;
use crate::llm::tasks::title::AutoTitleProfile;

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::OpenRouter,
        label: "OpenRouter",
        instructions: None,
        matcher: provider::matches,
        provider_policy: None,
        task_policy: ProfileTaskPolicyDescriptor {
            auto_title: Some(ProfileAutoTitlePolicyDescriptor::current(
                AutoTitleProfile::Common,
            )),
            compact_execution: Some(CompactExecutionPolicy::LocalPrompt),
        },
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 800,
    }
}
