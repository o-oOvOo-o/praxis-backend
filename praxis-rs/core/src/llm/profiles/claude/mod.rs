mod provider;

use super::plugin::ProfileDescriptor;
use super::plugin::ProfileTaskPolicyDescriptor;
use super::plugin::ProfileToolCapabilityDescriptor;
use crate::llm::ids::BehaviorProfileId;

pub(crate) fn profile() -> ProfileDescriptor {
    ProfileDescriptor {
        id: BehaviorProfileId::Claude,
        label: "Claude",
        instructions: None,
        prompt_layers: &[],
        matcher: provider::matches,
        provider_policy: None,
        task_policy: ProfileTaskPolicyDescriptor::local_prompt(),
        tool_capabilities: ProfileToolCapabilityDescriptor::praxis_web_search(),
        priority: 600,
    }
}
