use super::super::super::*;

pub(in crate::agent::control) fn listed_agent_next_action(
    recommended_target: &str,
    status: &AgentStatus,
) -> String {
    match status {
        AgentStatus::PendingInit | AgentStatus::Running => {
            format!(
                "Call wait_agent with target `{recommended_target}` only if this worker result is on the critical path."
            )
        }
        AgentStatus::Completed(_) | AgentStatus::Interrupted => {
            format!(
                "Inspect the result, then use assign_task with target `{recommended_target}` for another turn or close_agent when done."
            )
        }
        AgentStatus::Errored(_) => {
            format!(
                "Inspect the error, then use assign_task with target `{recommended_target}` to retry with a narrower task or close_agent."
            )
        }
        AgentStatus::Shutdown | AgentStatus::NotFound => {
            "Do not target this worker for new work; spawn or resume another worker if needed."
                .to_string()
        }
    }
}
