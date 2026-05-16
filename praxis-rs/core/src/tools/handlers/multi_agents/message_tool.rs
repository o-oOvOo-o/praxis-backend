//! Shared argument parsing and dispatch for text-only agent messaging tools.
//!
//! `send_message` and `assign_task` share the same submission path and differ only in whether the
//! resulting `InterAgentCommunication` should wake the target immediately.

use super::*;
use crate::agent_os::ResourceRequirement;
use crate::agent_os::RuntimeCommandType;
use crate::agent_os::TaskCreateRequest;
use praxis_protocol::protocol::InterAgentCommunication;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageDeliveryMode {
    QueueOnly,
    TriggerTurn,
}

impl MessageDeliveryMode {
    fn interaction_kind(self) -> CollabAgentInteractionKind {
        match self {
            Self::QueueOnly => CollabAgentInteractionKind::SendMessage,
            Self::TriggerTurn => CollabAgentInteractionKind::AssignTask,
        }
    }

    /// Returns whether the produced communication should start a turn immediately.
    fn apply(self, communication: InterAgentCommunication) -> InterAgentCommunication {
        match self {
            Self::QueueOnly => InterAgentCommunication {
                trigger_turn: false,
                ..communication
            },
            Self::TriggerTurn => InterAgentCommunication {
                trigger_turn: true,
                ..communication
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Input for the `send_message` tool.
pub(crate) struct SendMessageArgs {
    pub(crate) target: String,
    pub(crate) message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Input for the `assign_task` tool.
pub(crate) struct AssignTaskArgs {
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) message: Option<String>,
    #[serde(default)]
    pub(crate) objective: Option<String>,
    #[serde(default)]
    pub(crate) scope: Vec<String>,
    #[serde(default)]
    pub(crate) constraints: Vec<String>,
    #[serde(default)]
    pub(crate) acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub(crate) artifact_refs: Vec<String>,
    #[serde(default)]
    pub(crate) required_capabilities: Vec<String>,
    #[serde(default)]
    pub(crate) required_resources: Vec<String>,
    #[serde(default)]
    pub(crate) token_budget: Option<u64>,
    #[serde(default)]
    pub(crate) priority: Option<i32>,
    #[serde(default)]
    pub(crate) exploratory: bool,
    #[serde(default)]
    pub(crate) interrupt: bool,
}

#[derive(Debug, Serialize)]
/// Tool result shared by the message-delivery tools.
pub(crate) struct MessageToolResult {
    submission_id: String,
}

impl ToolOutput for MessageToolResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "multi_agent_message")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "multi_agent_message")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "multi_agent_message")
    }
}

fn message_content(message: String) -> Result<String, FunctionCallError> {
    if message.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "Empty message can't be sent to an agent".to_string(),
        ));
    }
    Ok(message)
}

/// Handles the shared plain-text message flow for both `send_message` and `assign_task`.
pub(crate) async fn handle_message_string_tool(
    invocation: ToolInvocation,
    mode: MessageDeliveryMode,
    target: String,
    message: String,
    interrupt: bool,
) -> Result<MessageToolResult, FunctionCallError> {
    handle_message_submission(
        invocation,
        mode,
        target,
        message_content(message)?,
        interrupt,
        None,
    )
    .await
}

pub(crate) async fn handle_assign_task_tool(
    invocation: ToolInvocation,
    args: AssignTaskArgs,
) -> Result<MessageToolResult, FunctionCallError> {
    let objective = args
        .objective
        .clone()
        .or_else(|| args.message.clone())
        .ok_or_else(|| {
            FunctionCallError::RespondToModel(
                "`assign_task` requires `objective` or legacy `message`".to_string(),
            )
        })
        .and_then(message_content)?;
    let prompt = args
        .message
        .map(message_content)
        .transpose()?
        .unwrap_or_else(|| objective.clone());
    if args.scope.is_empty() && !args.exploratory {
        return Err(FunctionCallError::RespondToModel(
            "`assign_task.scope` must be non-empty unless `exploratory` is true".to_string(),
        ));
    }
    let required_resources = parse_required_resources(&args.required_resources)?;
    handle_message_submission(
        invocation,
        MessageDeliveryMode::TriggerTurn,
        args.target,
        prompt,
        args.interrupt,
        Some(StructuredTaskInput {
            objective,
            scope: args.scope,
            constraints: args.constraints,
            acceptance_criteria: args.acceptance_criteria,
            artifact_refs: args.artifact_refs,
            required_capabilities: args.required_capabilities,
            required_resources,
            token_budget: args.token_budget,
            priority: args.priority.unwrap_or(0),
            exploratory: args.exploratory,
        }),
    )
    .await
}

struct StructuredTaskInput {
    objective: String,
    scope: Vec<String>,
    constraints: Vec<String>,
    acceptance_criteria: Vec<String>,
    artifact_refs: Vec<String>,
    required_capabilities: Vec<String>,
    required_resources: Vec<ResourceRequirement>,
    token_budget: Option<u64>,
    priority: i32,
    exploratory: bool,
}

async fn handle_message_submission(
    invocation: ToolInvocation,
    mode: MessageDeliveryMode,
    target: String,
    prompt: String,
    interrupt: bool,
    structured_task: Option<StructuredTaskInput>,
) -> Result<MessageToolResult, FunctionCallError> {
    let ToolInvocation {
        session,
        turn,
        payload,
        call_id,
        ..
    } = invocation;
    let _ = payload;
    let receiver_thread_id = resolve_agent_target(&session, &turn, &target).await?;
    let receiver_agent = session
        .services
        .agent_control
        .get_live_agent_metadata(receiver_thread_id)
        .await
        .unwrap_or_default();
    if mode == MessageDeliveryMode::TriggerTurn
        && receiver_agent
            .agent_path
            .as_ref()
            .is_some_and(AgentPath::is_root)
    {
        return Err(FunctionCallError::RespondToModel(
            "Tasks can't be assigned to the root agent".to_string(),
        ));
    }
    session
        .services
        .agent_os
        .ensure_inter_thread_message_allowed(
            session.conversation_id,
            receiver_thread_id,
            mode == MessageDeliveryMode::TriggerTurn,
        )
        .await
        .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
    if interrupt {
        session
            .services
            .agent_control
            .interrupt_agent(receiver_thread_id)
            .await
            .map_err(|err| collab_agent_error(receiver_thread_id, err))?;
    }
    let task_id = if mode == MessageDeliveryMode::TriggerTurn {
        let task = structured_task.unwrap_or_else(|| StructuredTaskInput {
            objective: prompt.clone(),
            scope: Vec::new(),
            constraints: Vec::new(),
            acceptance_criteria: Vec::new(),
            artifact_refs: Vec::new(),
            required_capabilities: Vec::new(),
            required_resources: Vec::new(),
            token_budget: None,
            priority: 0,
            exploratory: true,
        });
        let task_id = session
            .services
            .agent_os
            .create_task(TaskCreateRequest {
                objective: task.objective.clone(),
                scope: task.scope.clone(),
                constraints: task.constraints.clone(),
                acceptance_criteria: task.acceptance_criteria.clone(),
                artifact_refs: task.artifact_refs.clone(),
                priority: task.priority,
                assigned_thread_id: Some(receiver_thread_id),
                required_capabilities: task.required_capabilities.clone(),
                required_resources: task.required_resources.clone(),
                token_budget: task.token_budget,
                exploratory: task.exploratory,
                created_by: session.conversation_id,
            })
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
        session
            .services
            .agent_os
            .assign_task(task_id.as_str(), receiver_thread_id)
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
        let _runtime_command_id = session
            .services
            .agent_os
            .issue_runtime_command(
                session.conversation_id,
                receiver_thread_id,
                RuntimeCommandType::AssignTask,
                serde_json::json!({
                    "task_id": &task_id,
                    "objective": &task.objective,
                    "scope": &task.scope,
                    "constraints": &task.constraints,
                    "acceptance_criteria": &task.acceptance_criteria,
                    "artifact_refs": &task.artifact_refs,
                    "required_capabilities": &task.required_capabilities,
                    "required_resources": task
                        .required_resources
                        .iter()
                        .map(resource_requirement_name)
                        .collect::<Vec<_>>(),
                    "token_budget": task.token_budget,
                    "priority": task.priority,
                    "exploratory": task.exploratory,
                    "prompt": &prompt,
                    "interrupt": interrupt,
                }),
            )
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
        Some(task_id)
    } else {
        None
    };
    session
        .send_event(
            &turn,
            CollabAgentInteractionBeginEvent {
                call_id: call_id.clone(),
                sender_thread_id: session.conversation_id,
                receiver_thread_id,
                kind: mode.interaction_kind(),
                prompt: prompt.clone(),
            }
            .into(),
        )
        .await;
    let receiver_agent_path = receiver_agent.agent_path.clone().ok_or_else(|| {
        FunctionCallError::RespondToModel("target agent is missing an agent_path".to_string())
    })?;
    let communication = InterAgentCommunication::new(
        turn.session_source
            .get_agent_path()
            .unwrap_or_else(AgentPath::root),
        receiver_agent_path,
        Vec::new(),
        prompt.clone(),
        /*trigger_turn*/ true,
    );
    let result = session
        .services
        .agent_control
        .send_inter_agent_communication(receiver_thread_id, mode.apply(communication))
        .await
        .map_err(|err| collab_agent_error(receiver_thread_id, err));
    let status = session
        .services
        .agent_control
        .get_status(receiver_thread_id)
        .await;
    session
        .send_event(
            &turn,
            CollabAgentInteractionEndEvent {
                call_id,
                sender_thread_id: session.conversation_id,
                receiver_thread_id,
                kind: mode.interaction_kind(),
                receiver_agent_nickname: receiver_agent.agent_nickname,
                receiver_agent_role: receiver_agent.agent_role,
                prompt,
                status,
            }
            .into(),
        )
        .await;
    let submission_id = result?;
    if let Some(task_id) = task_id {
        tracing::debug!(%task_id, %receiver_thread_id, "AgentOS task assigned through multi-agent tool");
    }

    Ok(MessageToolResult { submission_id })
}

fn parse_required_resources(
    resources: &[String],
) -> Result<Vec<ResourceRequirement>, FunctionCallError> {
    resources
        .iter()
        .map(|resource| parse_required_resource(resource))
        .collect()
}

fn parse_required_resource(resource: &str) -> Result<ResourceRequirement, FunctionCallError> {
    let resource = resource.trim();
    if resource.is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "`assign_task.required_resources` cannot contain empty entries".to_string(),
        ));
    }
    let (kind, scope) = resource
        .split_once(':')
        .map(|(kind, scope)| (kind.trim(), Some(scope.trim())))
        .unwrap_or((resource, None));
    match kind {
        "cpu_heavy" => Ok(ResourceRequirement::CpuHeavy),
        "build_cache" => Ok(ResourceRequirement::BuildCache {
            scope: non_empty_scope(resource, scope)?,
        }),
        "app_runtime" => Ok(ResourceRequirement::AppRuntime {
            scope: non_empty_scope(resource, scope)?,
        }),
        "port" => {
            let port = non_empty_scope(resource, scope)?
                .parse::<u16>()
                .map_err(|_| {
                    FunctionCallError::RespondToModel(format!(
                        "`assign_task.required_resources` port must be a u16: `{resource}`"
                    ))
                })?;
            Ok(ResourceRequirement::Port { port })
        }
        "repo_write" | "file_write" => Ok(ResourceRequirement::RepoWrite {
            scope: non_empty_scope(resource, scope)?,
        }),
        "llm_budget" => Ok(ResourceRequirement::LlmBudget {
            scope: optional_scope(scope, "task"),
        }),
        "gpu" => Ok(ResourceRequirement::Gpu {
            scope: optional_scope(scope, "default"),
        }),
        "network" => Ok(ResourceRequirement::Network {
            scope: optional_scope(scope, "default"),
        }),
        "git_index" => Ok(ResourceRequirement::GitIndex {
            scope: non_empty_scope(resource, scope)?,
        }),
        _ => Err(FunctionCallError::RespondToModel(format!(
            "unknown `assign_task.required_resources` entry `{resource}`"
        ))),
    }
}

fn non_empty_scope(resource: &str, scope: Option<&str>) -> Result<String, FunctionCallError> {
    let Some(scope) = scope.filter(|scope| !scope.is_empty()) else {
        return Err(FunctionCallError::RespondToModel(format!(
            "`assign_task.required_resources` entry `{resource}` requires a scope"
        )));
    };
    Ok(scope.to_string())
}

fn optional_scope(scope: Option<&str>, default_scope: &str) -> String {
    scope
        .filter(|scope| !scope.is_empty())
        .unwrap_or(default_scope)
        .to_string()
}

fn resource_requirement_name(resource: &ResourceRequirement) -> String {
    match resource {
        ResourceRequirement::CpuHeavy => "cpu_heavy".to_string(),
        ResourceRequirement::BuildCache { scope } => format!("build_cache:{scope}"),
        ResourceRequirement::AppRuntime { scope } => format!("app_runtime:{scope}"),
        ResourceRequirement::Port { port } => format!("port:{port}"),
        ResourceRequirement::RepoWrite { scope } => format!("repo_write:{scope}"),
        ResourceRequirement::LlmBudget { scope } => format!("llm_budget:{scope}"),
        ResourceRequirement::Gpu { scope } => format!("gpu:{scope}"),
        ResourceRequirement::Network { scope } => format!("network:{scope}"),
        ResourceRequirement::GitIndex { scope } => format!("git_index:{scope}"),
    }
}
