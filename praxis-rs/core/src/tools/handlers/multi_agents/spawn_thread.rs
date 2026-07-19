use super::*;
use crate::agent::SpawnAgentOptions;
use crate::agent::next_thread_spawn_depth;
use crate::tools::context::FunctionToolOutput;
use praxis_protocol::protocol::AgentRank;
use praxis_protocol::protocol::ThreadGoal;

pub(crate) struct Handler;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnThreadArgs {
    objective: String,
    task_name: String,
    title: String,
    token_budget: Option<i64>,
}

#[derive(Debug, Serialize)]
struct SpawnThreadResult {
    thread_id: String,
    task_name: String,
    title: String,
    rank: String,
    role: String,
    goal: ThreadGoal,
    model: String,
    execution: &'static str,
}

#[async_trait]
impl ToolHandler for Handler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn effects(&self, invocation: &ToolInvocation) -> ToolEffects {
        ToolEffects::write(crate::tools::effects::conversation_effect_key(
            invocation.session.conversation_id,
            ["agents", "spawn", invocation.call_id.as_str()],
        ))
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let args: SpawnThreadArgs = parse_arguments(&function_arguments(payload)?)?;
        let caller_rank = turn.session_source.agent_rank_kind();
        let target_rank = target_thread_rank(caller_rank)?;
        let objective = required_text(args.objective, "objective")?;
        let task_name = required_text(args.task_name, "task_name")?;
        let title = required_text(args.title, "title")?;
        if args.token_budget.is_some_and(|budget| budget <= 0) {
            return Err(FunctionCallError::RespondToModel(
                "token_budget must be positive when provided".to_string(),
            ));
        }

        let mut config = build_agent_spawn_config(turn.as_ref())?;
        apply_spawn_agent_runtime_overrides(&mut config, turn.as_ref())?;
        let model = config
            .model
            .clone()
            .unwrap_or_else(|| turn.model_info.slug.clone());
        let depth = next_thread_spawn_depth(&turn.session_source);
        let source = thread_spawn_source(
            session.conversation_id,
            &turn.session_source,
            depth,
            Some(target_rank.agent_role()),
            Some(task_name.clone()),
            Some(title.clone()),
        )?;
        let spawned = session
            .services
            .agent_control
            .spawn_agent_thread(
                config,
                source,
                SpawnAgentOptions {
                    agent_title: Some(title.clone()),
                    dynamic_tools: turn.dynamic_tools.clone(),
                    ..Default::default()
                },
            )
            .await
            .map_err(collab_spawn_error)?;
        let goal = match spawned
            .thread
            .set_thread_goal_from_user(objective, args.token_budget.map(Some))
            .await
        {
            Ok(goal) => goal,
            Err(err) => {
                let _ = session
                    .services
                    .agent_control
                    .close_agent(spawned.thread_id)
                    .await;
                return Err(FunctionCallError::RespondToModel(format!(
                    "failed to initialize managed thread goal: {err:#}"
                )));
            }
        };
        turn.session_telemetry
            .counter("praxis.thread.spawn", 1, &[("rank", target_rank.id())]);
        let result = SpawnThreadResult {
            thread_id: spawned.thread_id.to_string(),
            task_name,
            title,
            rank: target_rank.id().to_string(),
            role: target_rank.agent_role().to_string(),
            goal,
            model,
            execution: "scheduled",
        };
        let body = serde_json::to_string_pretty(&result)
            .map_err(|err| FunctionCallError::Fatal(err.to_string()))?;
        Ok(FunctionToolOutput::from_text(body, Some(true)))
    }
}

fn target_thread_rank(caller_rank: AgentRank) -> Result<AgentRank, FunctionCallError> {
    caller_rank.managed_child_rank().ok_or_else(|| {
        FunctionCallError::RespondToModel(
            "R2 workers cannot spawn managed threads; use spawn_agent for an ordinary subagent"
                .to_string(),
        )
    })
}

fn required_text(value: String, field: &str) -> Result<String, FunctionCallError> {
    let value = value.trim();
    if value.is_empty() {
        Err(FunctionCallError::RespondToModel(format!(
            "{field} cannot be empty"
        )))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_thread_rank_is_derived_from_caller() {
        assert_eq!(
            target_thread_rank(AgentRank::Rank0).unwrap(),
            AgentRank::Rank1
        );
        assert_eq!(
            target_thread_rank(AgentRank::Rank1).unwrap(),
            AgentRank::Rank2
        );
        assert!(target_thread_rank(AgentRank::Rank2).is_err());
    }
}
