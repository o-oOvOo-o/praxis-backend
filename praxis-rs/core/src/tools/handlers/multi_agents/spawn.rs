use super::*;
use crate::agent::control::SpawnAgentForkMode;
use crate::agent::control::SpawnAgentOptions;
use crate::agent::control::render_input_preview;
use crate::agent::next_thread_spawn_depth;
use crate::agent::role::DEFAULT_ROLE_NAME;
use crate::agent::role::apply_role_to_config;
use serde::de;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = SpawnAgentResult;

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
            call_id,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: SpawnAgentArgs = parse_arguments(&arguments)?;
        let fork_mode = args.fork_mode()?;
        let display_title = args.display_title()?;
        let role_name = args
            .agent_type
            .as_deref()
            .map(str::trim)
            .filter(|role| !role.is_empty());

        let initial_operation = parse_collab_input(Some(args.message), /*items*/ None)?;
        let prompt = render_input_preview(&initial_operation);

        let session_source = turn.session_source.clone();
        let child_depth = next_thread_spawn_depth(&session_source);
        let max_depth = turn.config.agent_max_depth;
        if exceeds_thread_spawn_depth_limit(child_depth, max_depth) {
            return Err(FunctionCallError::RespondToModel(
                "Agent depth limit reached. Solve the task yourself.".to_string(),
            ));
        }
        session
            .send_event(
                &turn,
                CollabAgentSpawnBeginEvent {
                    call_id: call_id.clone(),
                    sender_thread_id: session.conversation_id,
                    prompt: prompt.clone(),
                    model: args
                        .model
                        .clone()
                        .or_else(|| args.model_provider.clone())
                        .unwrap_or_default(),
                    reasoning_effort: args.reasoning_effort.unwrap_or_default(),
                }
                .into(),
            )
            .await;
        let mut config = build_agent_spawn_config(turn.as_ref())?;
        apply_requested_spawn_agent_model_overrides(
            &session,
            turn.as_ref(),
            &mut config,
            args.model_provider.as_deref(),
            args.model.as_deref(),
            args.reasoning_effort,
        )
        .await?;
        apply_role_to_config(&mut config, role_name)
            .await
            .map_err(FunctionCallError::RespondToModel)?;
        apply_spawn_agent_runtime_overrides(&mut config, turn.as_ref())?;
        apply_spawn_agent_overrides(&mut config, child_depth);

        let spawn_source = thread_spawn_source(
            session.conversation_id,
            &turn.session_source,
            child_depth,
            role_name,
            Some(args.task_name.clone()),
            Some(display_title.clone()),
        )?;
        let result = session
            .services
            .agent_control
            .spawn_agent_with_metadata(
                config,
                initial_operation,
                Some(spawn_source),
                SpawnAgentOptions {
                    fork_parent_spawn_call_id: fork_mode.as_ref().map(|_| call_id.clone()),
                    fork_mode,
                    agent_title: Some(display_title),
                },
            )
            .await
            .map_err(collab_spawn_error);
        let (new_thread_id, new_agent_metadata, status) = match &result {
            Ok(spawned_agent) => (
                Some(spawned_agent.thread_id),
                Some(spawned_agent.metadata.clone()),
                spawned_agent.status.clone(),
            ),
            Err(_) => (None, None, AgentStatus::NotFound),
        };
        let agent_snapshot = match new_thread_id {
            Some(thread_id) => {
                session
                    .services
                    .agent_control
                    .get_agent_config_snapshot(thread_id)
                    .await
            }
            None => None,
        };
        let (new_agent_path, new_agent_base_name, new_agent_title, new_agent_display_name, new_agent_role) =
            match (&agent_snapshot, new_agent_metadata) {
                (Some(snapshot), _) => (
                    snapshot.session_source.get_agent_path().map(String::from),
                    snapshot.session_source.get_agent_base_name(),
                    snapshot.session_source.get_agent_title(),
                    snapshot.session_source.get_agent_display_name(),
                    snapshot.session_source.get_agent_role(),
                ),
                (None, Some(metadata)) => (
                    metadata.agent_path.map(String::from),
                    metadata.agent_base_name,
                    metadata.agent_title,
                    metadata.agent_display_name,
                    metadata.agent_role,
                ),
                (None, None) => (None, None, None, None, None),
            };
        let effective_model = agent_snapshot
            .as_ref()
            .map(|snapshot| snapshot.model.clone())
            .unwrap_or_else(|| args.model.clone().unwrap_or_default());
        let effective_reasoning_effort = agent_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.reasoning_effort)
            .unwrap_or(args.reasoning_effort.unwrap_or_default());
        let result_agent_base_name = new_agent_base_name.clone();
        let result_agent_title = new_agent_title.clone();
        let result_agent_display_name = new_agent_display_name.clone();
        session
            .send_event(
                &turn,
                CollabAgentSpawnEndEvent {
                    call_id,
                    sender_thread_id: session.conversation_id,
                    new_thread_id,
                    new_agent_base_name,
                    new_agent_title,
                    new_agent_display_name,
                    new_agent_role,
                    prompt,
                    model: effective_model,
                    reasoning_effort: effective_reasoning_effort,
                    status,
                }
                .into(),
            )
            .await;
        let _ = result?;
        let role_tag = role_name.unwrap_or(DEFAULT_ROLE_NAME);
        turn.session_telemetry.counter(
            "codex.multi_agent.spawn",
            /*inc*/ 1,
            &[("role", role_tag)],
        );
        let task_name = new_agent_path.ok_or_else(|| {
            FunctionCallError::RespondToModel(
                "spawned agent is missing a canonical task name".to_string(),
            )
        })?;

        Ok(SpawnAgentResult {
            agent_id: None,
            task_name,
            agent_base_name: result_agent_base_name,
            agent_title: result_agent_title,
            agent_display_name: result_agent_display_name,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnAgentArgs {
    message: String,
    task_name: String,
    title: String,
    agent_type: Option<String>,
    model_provider: Option<String>,
    model: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_reasoning_effort")]
    reasoning_effort: Option<ReasoningEffort>,
    fork_turns: Option<String>,
}

impl SpawnAgentArgs {
    fn display_title(&self) -> Result<String, FunctionCallError> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "title must be a short human-facing responsibility label".to_string(),
            ));
        }
        Ok(title.to_string())
    }

    fn fork_mode(&self) -> Result<Option<SpawnAgentForkMode>, FunctionCallError> {
        let Some(fork_turns) = self
            .fork_turns
            .as_deref()
            .map(str::trim)
            .filter(|fork_turns| !fork_turns.is_empty())
        else {
            return Ok(None);
        };

        if fork_turns.eq_ignore_ascii_case("none") {
            return Ok(None);
        }
        if fork_turns.eq_ignore_ascii_case("all") {
            return Ok(Some(SpawnAgentForkMode::FullHistory));
        }

        let last_n_turns = fork_turns.parse::<usize>().map_err(|_| {
            FunctionCallError::RespondToModel(
                "fork_turns must be `none`, `all`, or a positive integer string".to_string(),
            )
        })?;
        if last_n_turns == 0 {
            return Err(FunctionCallError::RespondToModel(
                "fork_turns must be `none`, `all`, or a positive integer string".to_string(),
            ));
        }

        Ok(Some(SpawnAgentForkMode::LastNTurns(last_n_turns)))
    }
}

fn deserialize_optional_reasoning_effort<'de, D>(
    deserializer: D,
) -> Result<Option<ReasoningEffort>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .as_deref()
        .map(parse_spawn_agent_reasoning_effort)
        .transpose()
        .map_err(de::Error::custom)
}

fn parse_spawn_agent_reasoning_effort(value: &str) -> Result<ReasoningEffort, String> {
    let compact = value
        .trim()
        .chars()
        .filter(|ch| !matches!(ch, '-' | '_' | ' '))
        .collect::<String>()
        .to_ascii_lowercase();
    match compact.as_str() {
        "" => Err("reasoning_effort can't be empty".to_string()),
        "none" | "off" | "false" => Ok(ReasoningEffort::None),
        "minimal" | "min" => Ok(ReasoningEffort::Minimal),
        "low" => Ok(ReasoningEffort::Low),
        "medium" | "med" | "default" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        "xhigh" | "extrahigh" | "max" | "maximum" | "highest" => Ok(ReasoningEffort::XHigh),
        _ => value.parse(),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SpawnAgentResult {
    agent_id: Option<String>,
    task_name: String,
    agent_base_name: Option<String>,
    agent_title: Option<String>,
    agent_display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_agent_args_accepts_reasoning_aliases() {
        let args: SpawnAgentArgs = serde_json::from_str(
            r#"{"message":"do it","task_name":"worker","title":"负责实现","reasoning_effort":"x-high"}"#,
        )
        .expect("x-high effort should parse");
        assert_eq!(args.reasoning_effort, Some(ReasoningEffort::XHigh));

        let args: SpawnAgentArgs = serde_json::from_str(
            r#"{"message":"do it","task_name":"worker","title":"负责实现","reasoning_effort":"maximum"}"#,
        )
        .expect("maximum effort should parse");
        assert_eq!(args.reasoning_effort, Some(ReasoningEffort::XHigh));
    }
}

impl ToolOutput for SpawnAgentResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "spawn_agent")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "spawn_agent")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "spawn_agent")
    }
}
