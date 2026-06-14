use super::*;
use crate::agent::control::ListedAgent;
use crate::agent_os::AgentOsSnapshot;
use crate::agent_os::AgentOsSnapshotOptions;
use crate::tools::loop_guard::ToolLoopDecision;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = ListAgentsResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: ListAgentsArgs = parse_arguments(&arguments)?;
        session
            .services
            .agent_control
            .register_session_root(session.conversation_id, &turn.session_source);
        let raw_agents = session
            .services
            .agent_control
            .list_agents(
                session.conversation_id,
                &turn.session_source,
                args.path_prefix.as_deref(),
            )
            .await
            .map_err(collab_spawn_error)?;
        let only_root = raw_agents.len() == 1 && raw_agents[0].agent_name == "/root";
        let agents = raw_agents
            .into_iter()
            .filter(|agent| agent.agent_name != "/root")
            .collect::<Vec<_>>();

        let agent_os = session
            .services
            .agent_os
            .snapshot(AgentOsSnapshotOptions::default())
            .await;

        let terminal_state = ListAgentsTerminalState::from_snapshot(only_root, &agents, &agent_os);
        if let ToolLoopDecision::Block { message } = turn
            .tool_loop_guard
            .record_list_agents_terminal(terminal_state.should_stop_listing)
        {
            return Err(FunctionCallError::RespondToModel(message));
        }

        Ok(ListAgentsResult {
            agents,
            agent_os,
            terminal_state,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListAgentsArgs {
    path_prefix: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAgentsResult {
    agents: Vec<ListedAgent>,
    agent_os: AgentOsSnapshot,
    terminal_state: ListAgentsTerminalState,
}

#[derive(Debug, Serialize)]
struct ListAgentsTerminalState {
    only_root: bool,
    no_live_subagents: bool,
    no_pending_agent_os_work: bool,
    should_stop_listing: bool,
    message: String,
}

impl ListAgentsTerminalState {
    fn from_snapshot(only_root: bool, agents: &[ListedAgent], agent_os: &AgentOsSnapshot) -> Self {
        let no_live_subagents = agents.is_empty();
        let no_pending_agent_os_work = agent_os.no_pending_work();
        let should_stop_listing = no_live_subagents && no_pending_agent_os_work;
        let message = if should_stop_listing {
            "No live sub-agents remain and AgentOS has no pending work; stop calling list_agents and summarize."
        } else if no_live_subagents {
            "No live sub-agents remain, but AgentOS still reports pending work."
        } else {
            "Live sub-agents remain; use targeted wait_agent, assign_task, or close_agent as needed."
        }
        .to_string();
        Self {
            only_root,
            no_live_subagents,
            no_pending_agent_os_work,
            should_stop_listing,
            message,
        }
    }
}

impl ToolOutput for ListAgentsResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "list_agents")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "list_agents")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "list_agents")
    }
}
