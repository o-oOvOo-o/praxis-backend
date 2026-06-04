use crate::function_tool::FunctionCallError;
use crate::goals::CreateGoalRequest;
use crate::goals::GoalRuntimeEvent;
use crate::goals::SetGoalRequest;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use praxis_protocol::protocol::ThreadGoal;
use praxis_protocol::protocol::ThreadGoalStatus;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Write as _;

use super::parse_arguments;

pub struct CreateGoalHandler;
pub struct GetGoalHandler;
pub struct UpdateGoalHandler;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CreateGoalArgs {
    objective: String,
    token_budget: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct UpdateGoalArgs {
    status: ThreadGoalStatus,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoalToolResponse {
    goal: Option<ThreadGoal>,
    remaining_tokens: Option<i64>,
}

impl GoalToolResponse {
    fn new(goal: Option<ThreadGoal>) -> Self {
        let remaining_tokens = goal.as_ref().and_then(|goal| {
            goal.token_budget
                .map(|budget| (budget - goal.tokens_used).max(0))
        });
        Self {
            goal,
            remaining_tokens,
        }
    }
}

#[async_trait]
impl ToolHandler for CreateGoalHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload, "create_goal")?;
        let args: CreateGoalArgs = parse_arguments(&arguments)?;
        let goal = session
            .create_thread_goal(
                turn.as_ref(),
                CreateGoalRequest {
                    objective: args.objective,
                    token_budget: args.token_budget,
                },
            )
            .await
            .map_err(|err| {
                if err
                    .chain()
                    .any(|cause| cause.to_string().contains("already has a goal"))
                {
                    FunctionCallError::RespondToModel(
                        "cannot create a new goal because this thread already has a goal; use update_goal only when the existing goal is complete"
                            .to_string(),
                    )
                } else {
                    FunctionCallError::RespondToModel(format_goal_error(err))
                }
            })?;
        goal_response(Some(goal))
    }
}

#[async_trait]
impl ToolHandler for GetGoalHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let _arguments = function_arguments(payload, "get_goal")?;
        let goal = session
            .get_thread_goal()
            .await
            .map_err(|err| FunctionCallError::RespondToModel(format_goal_error(err)))?;
        goal_response(goal)
    }
}

#[async_trait]
impl ToolHandler for UpdateGoalHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload, "update_goal")?;
        let args: UpdateGoalArgs = parse_arguments(&arguments)?;
        if !matches!(
            args.status,
            ThreadGoalStatus::Complete | ThreadGoalStatus::Blocked
        ) {
            return Err(FunctionCallError::RespondToModel(
                "update_goal can only mark the existing goal complete or blocked; pause, resume, budget-limited, and usage-limited status changes are controlled by the user or system"
                    .to_string(),
            ));
        }
        session
            .goal_runtime_apply(GoalRuntimeEvent::ToolCompletedGoal {
                turn_context: turn.as_ref(),
            })
            .await
            .map_err(|err| FunctionCallError::RespondToModel(format_goal_error(err)))?;
        let goal = session
            .set_thread_goal(
                turn.as_ref(),
                SetGoalRequest {
                    objective: None,
                    status: Some(args.status),
                    token_budget: None,
                },
            )
            .await
            .map_err(|err| FunctionCallError::RespondToModel(format_goal_error(err)))?;
        goal_response(Some(goal))
    }
}

fn function_arguments(
    payload: ToolPayload,
    handler_name: &'static str,
) -> Result<String, FunctionCallError> {
    match payload {
        ToolPayload::Function { arguments } => Ok(arguments),
        _ => Err(FunctionCallError::RespondToModel(format!(
            "{handler_name} handler received unsupported payload"
        ))),
    }
}

fn format_goal_error(err: anyhow::Error) -> String {
    let mut message = err.to_string();
    for cause in err.chain().skip(1) {
        let _ = write!(message, ": {cause}");
    }
    message
}

fn goal_response(goal: Option<ThreadGoal>) -> Result<FunctionToolOutput, FunctionCallError> {
    let response = serde_json::to_string_pretty(&GoalToolResponse::new(goal))
        .map_err(|err| FunctionCallError::Fatal(err.to_string()))?;
    Ok(FunctionToolOutput::from_text(response, Some(true)))
}
