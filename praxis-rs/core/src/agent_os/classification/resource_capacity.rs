use crate::agent_os::model::ResourceRequirement;

pub(in crate::agent_os) fn capacity_for_requirement(requirement: &ResourceRequirement) -> usize {
    match requirement {
        ResourceRequirement::CpuHeavy => 1,
        ResourceRequirement::LlmBudget { .. } => 8,
        _ => 1,
    }
}
