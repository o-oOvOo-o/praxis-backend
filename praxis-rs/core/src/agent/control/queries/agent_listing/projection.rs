use super::super::super::*;
use super::next_action::listed_agent_next_action;

pub(super) fn resolve_agent_list_prefix(
    current_session_source: &SessionSource,
    path_prefix: Option<&str>,
) -> PraxisResult<Option<AgentPath>> {
    path_prefix
        .map(|prefix| {
            current_session_source
                .get_agent_path()
                .unwrap_or_else(AgentPath::root)
                .resolve(prefix)
                .map_err(PraxisErr::UnsupportedOperation)
        })
        .transpose()
}

pub(super) fn should_include_agent(
    agent_path: Option<&AgentPath>,
    resolved_prefix: Option<&AgentPath>,
) -> bool {
    resolved_prefix.is_none_or(|prefix| agent_matches_prefix(agent_path, prefix))
}

pub(super) fn root_listed_agent(thread_id: ThreadId, agent_status: AgentStatus) -> ListedAgent {
    let root_path = AgentPath::root();
    ListedAgent {
        thread_id,
        agent_name: root_path.to_string(),
        agent_base_name: None,
        agent_title: None,
        agent_display_name: None,
        agent_role: None,
        agent_status,
        last_task_message: Some(ROOT_LAST_TASK_MESSAGE.to_string()),
        recommended_target: thread_id.to_string(),
        next_action:
            "This is the root thread; list_agents omits it from tool output for subagent coordination."
                .to_string(),
    }
}

pub(super) fn live_listed_agent(
    thread_id: ThreadId,
    metadata: AgentMetadata,
    agent_status: AgentStatus,
) -> ListedAgent {
    let recommended_target = thread_id.to_string();
    let next_action = listed_agent_next_action(&recommended_target, &agent_status);
    ListedAgent {
        thread_id,
        agent_name: metadata
            .agent_path
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| thread_id.to_string()),
        agent_base_name: metadata.agent_base_name,
        agent_title: metadata.agent_title,
        agent_display_name: metadata.agent_display_name,
        agent_role: metadata.agent_role,
        agent_status,
        last_task_message: metadata.last_task_message,
        recommended_target,
        next_action,
    }
}
