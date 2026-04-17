use crate::codex::Session;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::handlers::multi_agents_common::function_arguments;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use async_trait::async_trait;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::protocol::InterAgentCommunication;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) struct TeamReadHandler;
pub(crate) struct TeamSendMessageHandler;
pub(crate) struct TeamTaskCreateHandler;
pub(crate) struct TeamTaskListHandler;
pub(crate) struct TeamTaskUpdateHandler;

#[async_trait]
impl ToolHandler for TeamReadHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &crate::tools::context::ToolPayload) -> bool {
        matches!(payload, crate::tools::context::ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: TeamReadArgs = parse_arguments(&arguments)?;
        let db = required_state_db(&session)?;
        let context = resolve_team_context(&session, &db).await?;

        let teammates = if args.include_teammates.unwrap_or(true) {
            db.list_team_teammates(context.team.id.as_str())
                .await
                .map_err(|err| state_db_error("list team teammates", err))?
                .iter()
                .map(team_teammate_output_from_state)
                .collect()
        } else {
            Vec::new()
        };

        let tasks = if args.include_tasks.unwrap_or(true) {
            db.list_team_tasks(context.team.id.as_str())
                .await
                .map_err(|err| state_db_error("list team tasks", err))?
                .iter()
                .map(team_task_output_from_state)
                .collect()
        } else {
            Vec::new()
        };

        let messages = if args.include_messages.unwrap_or(true) {
            db.list_team_mailbox_messages(
                context.team.id.as_str(),
                Some(args.message_limit.unwrap_or(DEFAULT_TEAM_MESSAGE_LIMIT)),
            )
            .await
            .map_err(|err| state_db_error("list team mailbox messages", err))?
            .iter()
            .map(team_message_output_from_state)
            .collect()
        } else {
            Vec::new()
        };

        Ok(json_output(
            &TeamReadResult {
                team: team_output_from_state(&context.team),
                current_participant: participant_output_from_resolved(&context.participant),
                teammates,
                tasks,
                messages,
            },
            "team_read",
        ))
    }
}

#[async_trait]
impl ToolHandler for TeamSendMessageHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &crate::tools::context::ToolPayload) -> bool {
        matches!(payload, crate::tools::context::ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: TeamSendMessageArgs = parse_arguments(&arguments)?;
        let db = required_state_db(&session)?;
        let context = resolve_team_context(&session, &db).await?;
        let Some(body) = normalize_required_text(args.body.as_str()) else {
            return Err(FunctionCallError::RespondToModel(
                "team_send_message body must not be empty".to_string(),
            ));
        };

        let sender = participant_ref_from_resolved(&context.participant, &context.team);
        let recipient =
            resolve_message_recipient(&db, &context.team, args.recipient.as_str()).await?;

        if participants_equal(&sender.participant, &recipient.participant) {
            return Err(FunctionCallError::RespondToModel(
                "sender and recipient must be different".to_string(),
            ));
        }

        let (sender_kind, sender_teammate_id) = participant_ref_to_state(&sender.participant);
        let (recipient_kind, recipient_teammate_id) =
            participant_ref_to_state(&recipient.participant);
        let message = db
            .create_team_mailbox_message(&codex_state::TeamMailboxMessageCreateParams {
                id: new_message_id(),
                team_id: context.team.id.clone(),
                sender_kind,
                sender_teammate_id,
                recipient_kind,
                recipient_teammate_id,
                body,
            })
            .await
            .map_err(|err| state_db_error("create team mailbox message", err))?;
        let live_delivery = deliver_live_team_message(
            &session,
            &db,
            &context.team,
            &sender,
            &recipient,
            message.body.as_str(),
        )
        .await;

        Ok(json_output(
            &TeamSendMessageResult {
                message: team_message_output_with_participants(
                    &message,
                    participant_output_from_ref(&sender.participant),
                    participant_output_from_ref(&recipient.participant),
                ),
                live_delivery,
            },
            "team_send_message",
        ))
    }
}

#[async_trait]
impl ToolHandler for TeamTaskCreateHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &crate::tools::context::ToolPayload) -> bool {
        matches!(payload, crate::tools::context::ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: TeamTaskCreateArgs = parse_arguments(&arguments)?;
        let db = required_state_db(&session)?;
        let context = resolve_team_context(&session, &db).await?;

        let Some(title) = normalize_required_text(args.title.as_str()) else {
            return Err(FunctionCallError::RespondToModel(
                "team_task_create title must not be empty".to_string(),
            ));
        };
        let description = normalize_optional_text(args.description);
        let assignee_teammate_id = normalize_optional_text(args.assignee_teammate_id);
        if let Some(assignee_teammate_id) = assignee_teammate_id.as_ref() {
            require_teammate(
                &db,
                context.team.id.as_str(),
                assignee_teammate_id.as_str(),
                "assignee_teammate_id",
            )
            .await?;
        }

        let task = db
            .create_team_task(&codex_state::TeamTaskCreateParams {
                team_id: context.team.id.clone(),
                task_id: new_task_id(),
                title,
                description,
                status: codex_state::TeamTaskStatus::Pending,
                assignee_teammate_id,
            })
            .await
            .map_err(|err| state_db_error("create team task", err))?;

        Ok(json_output(
            &TeamTaskResult {
                task: team_task_output_from_state(&task),
            },
            "team_task_create",
        ))
    }
}

#[async_trait]
impl ToolHandler for TeamTaskListHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &crate::tools::context::ToolPayload) -> bool {
        matches!(payload, crate::tools::context::ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let _: TeamTaskListArgs = parse_arguments(&arguments)?;
        let db = required_state_db(&session)?;
        let context = resolve_team_context(&session, &db).await?;

        let tasks = db
            .list_team_tasks(context.team.id.as_str())
            .await
            .map_err(|err| state_db_error("list team tasks", err))?
            .iter()
            .map(team_task_output_from_state)
            .collect();

        Ok(json_output(
            &TeamTaskListResult { data: tasks },
            "team_task_list",
        ))
    }
}

#[async_trait]
impl ToolHandler for TeamTaskUpdateHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &crate::tools::context::ToolPayload) -> bool {
        matches!(payload, crate::tools::context::ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: TeamTaskUpdateArgs = parse_arguments(&arguments)?;
        let db = required_state_db(&session)?;
        let context = resolve_team_context(&session, &db).await?;

        let Some(task_id) = normalize_required_text(args.task_id.as_str()) else {
            return Err(FunctionCallError::RespondToModel(
                "team_task_update task_id must not be empty".to_string(),
            ));
        };
        let title = normalize_optional_text(args.title);
        let description = normalize_optional_text(args.description);
        let assignee_teammate_id = normalize_optional_text(args.assignee_teammate_id);
        if let Some(assignee_teammate_id) = assignee_teammate_id.as_ref()
            && !args.clear_assignee
        {
            require_teammate(
                &db,
                context.team.id.as_str(),
                assignee_teammate_id.as_str(),
                "assignee_teammate_id",
            )
            .await?;
        }

        let task = db
            .update_team_task(&codex_state::TeamTaskUpdateParams {
                team_id: context.team.id.clone(),
                task_id: task_id.clone(),
                title,
                description,
                status: args.status.map(Into::into),
                assignee_teammate_id,
                clear_assignee: args.clear_assignee,
            })
            .await
            .map_err(|err| state_db_error("update team task", err))?
            .ok_or_else(|| {
                FunctionCallError::RespondToModel(format!(
                    "team task not found in current team: {task_id}"
                ))
            })?;

        Ok(json_output(
            &TeamTaskResult {
                task: team_task_output_from_state(&task),
            },
            "team_task_update",
        ))
    }
}

const DEFAULT_TEAM_MESSAGE_LIMIT: usize = 20;

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TeamReadArgs {
    include_teammates: Option<bool>,
    include_tasks: Option<bool>,
    include_messages: Option<bool>,
    message_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamSendMessageArgs {
    recipient: String,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamTaskCreateArgs {
    title: String,
    description: Option<String>,
    assignee_teammate_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TeamTaskListArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamTaskUpdateArgs {
    task_id: String,
    title: Option<String>,
    description: Option<String>,
    status: Option<TeamTaskStatusValue>,
    assignee_teammate_id: Option<String>,
    #[serde(default)]
    clear_assignee: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ParticipantKind {
    Lead,
    Teammate,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TeamTaskStatusValue {
    Pending,
    InProgress,
    Blocked,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum TeamTeammateStatusValue {
    Pending,
    Active,
    Failed,
    Closed,
}

#[derive(Debug, Serialize)]
struct TeamReadResult {
    team: TeamOutput,
    current_participant: ParticipantOutput,
    teammates: Vec<TeamTeammateOutput>,
    tasks: Vec<TeamTaskOutput>,
    messages: Vec<TeamMessageOutput>,
}

#[derive(Debug, Serialize)]
struct TeamSendMessageResult {
    message: TeamMessageOutput,
    live_delivery: Option<TeamLiveDeliveryOutput>,
}

#[derive(Debug, Serialize)]
struct TeamTaskResult {
    task: TeamTaskOutput,
}

#[derive(Debug, Serialize)]
struct TeamTaskListResult {
    data: Vec<TeamTaskOutput>,
}

#[derive(Debug, Serialize)]
struct TeamOutput {
    team_id: String,
    lead_thread_id: String,
    name: String,
    objective: Option<String>,
}

#[derive(Debug, Serialize)]
struct ParticipantOutput {
    kind: ParticipantKind,
    teammate_id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct TeamTeammateOutput {
    team_id: String,
    teammate_id: String,
    name: String,
    role: Option<String>,
    status: TeamTeammateStatusValue,
    thread_id: Option<String>,
    last_error: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Serialize)]
struct TeamTaskOutput {
    team_id: String,
    task_id: String,
    title: String,
    description: Option<String>,
    status: TeamTaskStatusValue,
    assignee_teammate_id: Option<String>,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct TeamMessageOutput {
    message_id: String,
    team_id: String,
    sender: ParticipantOutput,
    recipient: ParticipantOutput,
    body: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct TeamLiveDeliveryOutput {
    target_thread_id: String,
    submission_id: String,
}

#[derive(Debug, Clone)]
struct TeamContext {
    team: codex_state::Team,
    participant: ResolvedParticipant,
}

#[derive(Debug, Clone)]
enum ResolvedParticipant {
    Lead,
    Teammate(codex_state::TeamTeammate),
}

#[derive(Debug, Clone)]
enum ParticipantRef {
    Lead,
    Teammate {
        teammate_id: String,
        name: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct ResolvedParticipantTarget {
    participant: ParticipantRef,
    live_thread_id: Option<String>,
}

impl From<TeamTaskStatusValue> for codex_state::TeamTaskStatus {
    fn from(value: TeamTaskStatusValue) -> Self {
        match value {
            TeamTaskStatusValue::Pending => Self::Pending,
            TeamTaskStatusValue::InProgress => Self::InProgress,
            TeamTaskStatusValue::Blocked => Self::Blocked,
            TeamTaskStatusValue::Completed => Self::Completed,
        }
    }
}

fn required_state_db(
    session: &Arc<Session>,
) -> Result<Arc<codex_state::StateRuntime>, FunctionCallError> {
    session.state_db().ok_or_else(|| {
        FunctionCallError::Fatal("sqlite state db is unavailable for this session".to_string())
    })
}

async fn resolve_team_context(
    session: &Arc<Session>,
    db: &Arc<codex_state::StateRuntime>,
) -> Result<TeamContext, FunctionCallError> {
    let current_thread_id = session.conversation_id.to_string();
    if let Some(team) = db
        .get_team_by_lead_thread_id(current_thread_id.as_str())
        .await
        .map_err(|err| state_db_error("load team by lead thread id", err))?
    {
        return Ok(TeamContext {
            team,
            participant: ResolvedParticipant::Lead,
        });
    }

    if let Some(teammate) = db
        .get_team_teammate_by_thread_id(current_thread_id.as_str())
        .await
        .map_err(|err| state_db_error("load team teammate by thread id", err))?
    {
        let team = db
            .get_team(teammate.team_id.as_str())
            .await
            .map_err(|err| state_db_error("load team for teammate", err))?
            .ok_or_else({
                let missing_team_id = teammate.team_id.clone();
                let missing_teammate_id = teammate.teammate_id.clone();
                move || {
                    FunctionCallError::Fatal(format!(
                        "team {} referenced by teammate {} is missing",
                        missing_team_id, missing_teammate_id
                    ))
                }
            })?;
        return Ok(TeamContext {
            team,
            participant: ResolvedParticipant::Teammate(teammate),
        });
    }

    Err(FunctionCallError::RespondToModel(
        "current thread is not attached to a team".to_string(),
    ))
}

async fn require_teammate(
    db: &Arc<codex_state::StateRuntime>,
    team_id: &str,
    teammate_id: &str,
    field_name: &str,
) -> Result<codex_state::TeamTeammate, FunctionCallError> {
    let Some(teammate_id) = normalize_required_text(teammate_id) else {
        return Err(FunctionCallError::RespondToModel(format!(
            "{field_name} must not be empty"
        )));
    };
    db.get_team_teammate(team_id, teammate_id.as_str())
        .await
        .map_err(|err| state_db_error("load team teammate", err))?
        .ok_or_else(|| {
            FunctionCallError::RespondToModel(format!(
                "teammate not found in current team: {teammate_id}"
            ))
        })
}

async fn resolve_message_recipient(
    db: &Arc<codex_state::StateRuntime>,
    team: &codex_state::Team,
    recipient: &str,
) -> Result<ResolvedParticipantTarget, FunctionCallError> {
    let Some(recipient) = normalize_required_text(recipient) else {
        return Err(FunctionCallError::RespondToModel(
            "recipient must not be empty".to_string(),
        ));
    };
    if recipient.eq_ignore_ascii_case("lead") {
        return Ok(ResolvedParticipantTarget {
            participant: ParticipantRef::Lead,
            live_thread_id: Some(team.lead_thread_id.clone()),
        });
    }

    let teammate = require_teammate(db, team.id.as_str(), recipient.as_str(), "recipient").await?;
    Ok(ResolvedParticipantTarget {
        live_thread_id: teammate.thread_id.clone(),
        participant: ParticipantRef::Teammate {
            teammate_id: teammate.teammate_id,
            name: Some(teammate.name),
        },
    })
}

fn participant_ref_from_resolved(
    participant: &ResolvedParticipant,
    team: &codex_state::Team,
) -> ResolvedParticipantTarget {
    match participant {
        ResolvedParticipant::Lead => ResolvedParticipantTarget {
            participant: ParticipantRef::Lead,
            live_thread_id: Some(team.lead_thread_id.clone()),
        },
        ResolvedParticipant::Teammate(teammate) => ResolvedParticipantTarget {
            live_thread_id: teammate.thread_id.clone(),
            participant: ParticipantRef::Teammate {
                teammate_id: teammate.teammate_id.clone(),
                name: Some(teammate.name.clone()),
            },
        },
    }
}

fn participant_ref_to_state(
    participant: &ParticipantRef,
) -> (codex_state::TeamMailboxParticipantKind, Option<String>) {
    match participant {
        ParticipantRef::Lead => (codex_state::TeamMailboxParticipantKind::Lead, None),
        ParticipantRef::Teammate { teammate_id, .. } => (
            codex_state::TeamMailboxParticipantKind::Teammate,
            Some(teammate_id.clone()),
        ),
    }
}

fn participants_equal(left: &ParticipantRef, right: &ParticipantRef) -> bool {
    match (left, right) {
        (ParticipantRef::Lead, ParticipantRef::Lead) => true,
        (
            ParticipantRef::Teammate {
                teammate_id: left_id,
                ..
            },
            ParticipantRef::Teammate {
                teammate_id: right_id,
                ..
            },
        ) => left_id.trim() == right_id.trim(),
        _ => false,
    }
}

fn json_output<T: Serialize>(value: &T, tool_name: &str) -> FunctionToolOutput {
    let text = serde_json::to_string(value).unwrap_or_else(|err| {
        serde_json::Value::String(format!("failed to serialize {tool_name} result: {err}"))
            .to_string()
    });
    FunctionToolOutput::from_text(text, Some(true))
}

fn state_db_error(context: &str, err: impl std::fmt::Display) -> FunctionCallError {
    FunctionCallError::Fatal(format!("failed to {context}: {err}"))
}

fn normalize_required_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| normalize_required_text(value.as_str()))
}

fn new_task_id() -> String {
    format!("task_{}", Uuid::now_v7().simple())
}

fn new_message_id() -> String {
    format!("msg_{}", Uuid::now_v7().simple())
}

fn team_output_from_state(team: &codex_state::Team) -> TeamOutput {
    TeamOutput {
        team_id: team.id.clone(),
        lead_thread_id: team.lead_thread_id.clone(),
        name: team.name.clone(),
        objective: team.objective.clone(),
    }
}

fn participant_output_from_resolved(participant: &ResolvedParticipant) -> ParticipantOutput {
    match participant {
        ResolvedParticipant::Lead => ParticipantOutput {
            kind: ParticipantKind::Lead,
            teammate_id: None,
            name: None,
        },
        ResolvedParticipant::Teammate(teammate) => ParticipantOutput {
            kind: ParticipantKind::Teammate,
            teammate_id: Some(teammate.teammate_id.clone()),
            name: Some(teammate.name.clone()),
        },
    }
}

fn participant_output_from_ref(participant: &ParticipantRef) -> ParticipantOutput {
    match participant {
        ParticipantRef::Lead => ParticipantOutput {
            kind: ParticipantKind::Lead,
            teammate_id: None,
            name: None,
        },
        ParticipantRef::Teammate { teammate_id, name } => ParticipantOutput {
            kind: ParticipantKind::Teammate,
            teammate_id: Some(teammate_id.clone()),
            name: name.clone(),
        },
    }
}

async fn deliver_live_team_message(
    session: &Arc<Session>,
    db: &Arc<codex_state::StateRuntime>,
    team: &codex_state::Team,
    sender: &ResolvedParticipantTarget,
    recipient: &ResolvedParticipantTarget,
    body: &str,
) -> Option<TeamLiveDeliveryOutput> {
    let target_thread_id = recipient.live_thread_id.as_ref()?;
    let target_thread_id = ThreadId::from_string(target_thread_id.as_str()).ok()?;
    let communication = InterAgentCommunication::new(
        resolve_participant_agent_path(session, db, team, sender).await,
        resolve_participant_agent_path(session, db, team, recipient).await,
        Vec::new(),
        format_live_team_message(&sender.participant, body),
        /*trigger_turn*/ true,
    );
    let submission_id = session
        .services
        .agent_control
        .send_inter_agent_communication(target_thread_id, communication)
        .await
        .ok()?;
    Some(TeamLiveDeliveryOutput {
        target_thread_id: target_thread_id.to_string(),
        submission_id,
    })
}

async fn resolve_participant_agent_path(
    session: &Arc<Session>,
    db: &Arc<codex_state::StateRuntime>,
    team: &codex_state::Team,
    participant: &ResolvedParticipantTarget,
) -> AgentPath {
    match &participant.participant {
        ParticipantRef::Lead => {
            resolve_team_thread_agent_path(session, db, team.lead_thread_id.as_str())
                .await
                .unwrap_or_else(AgentPath::root)
        }
        ParticipantRef::Teammate { teammate_id, .. } => {
            if let Some(thread_id) = participant.live_thread_id.as_deref()
                && let Some(agent_path) =
                    resolve_team_thread_agent_path(session, db, thread_id).await
            {
                return agent_path;
            }
            resolve_team_lead_spawn_path(session, db, team.lead_thread_id.as_str())
                .await
                .and_then(|lead_path| lead_path.join(teammate_id.as_str()).ok())
                .unwrap_or_else(AgentPath::root)
        }
    }
}

async fn resolve_team_thread_agent_path(
    session: &Arc<Session>,
    db: &Arc<codex_state::StateRuntime>,
    thread_id: &str,
) -> Option<AgentPath> {
    let thread_id = ThreadId::from_string(thread_id).ok()?;
    if let Some(snapshot) = session
        .services
        .agent_control
        .get_agent_config_snapshot(thread_id)
        .await
    {
        return Some(
            snapshot
                .session_source
                .get_agent_path()
                .unwrap_or_else(AgentPath::root),
        );
    }

    let metadata = db.get_thread(thread_id).await.ok().flatten()?;
    metadata
        .agent_path
        .as_deref()
        .and_then(|agent_path| AgentPath::try_from(agent_path).ok())
        .or_else(|| {
            parse_session_source_str(metadata.source.as_str())
                .and_then(|source| source.get_agent_path())
        })
        .or_else(|| Some(AgentPath::root()))
}

async fn resolve_team_lead_spawn_path(
    session: &Arc<Session>,
    db: &Arc<codex_state::StateRuntime>,
    lead_thread_id: &str,
) -> Option<AgentPath> {
    let thread_id = ThreadId::from_string(lead_thread_id).ok()?;
    if let Some(snapshot) = session
        .services
        .agent_control
        .get_agent_config_snapshot(thread_id)
        .await
    {
        return Some(
            snapshot
                .session_source
                .get_agent_path()
                .unwrap_or_else(AgentPath::root),
        );
    }

    let metadata = db.get_thread(thread_id).await.ok().flatten()?;
    metadata
        .agent_path
        .as_deref()
        .and_then(|agent_path| AgentPath::try_from(agent_path).ok())
        .or_else(|| {
            parse_session_source_str(metadata.source.as_str())
                .and_then(|source| source.get_agent_path())
        })
        .or_else(|| Some(AgentPath::root()))
}

fn format_live_team_message(sender: &ParticipantRef, body: &str) -> String {
    let sender_label = match sender {
        ParticipantRef::Lead => "team lead".to_string(),
        ParticipantRef::Teammate {
            name: Some(name), ..
        } => format!("teammate {name}"),
        ParticipantRef::Teammate { teammate_id, .. } => format!("teammate {teammate_id}"),
    };
    format!(
        "Team mailbox message from {sender_label}. Use team_read to inspect the latest team state if needed.\n\n{body}"
    )
}

fn parse_session_source_str(source: &str) -> Option<codex_protocol::protocol::SessionSource> {
    serde_json::from_str(source)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(source.to_string())))
        .ok()
}

fn participant_output_from_state(
    kind: codex_state::TeamMailboxParticipantKind,
    teammate_id: Option<&String>,
) -> ParticipantOutput {
    match kind {
        codex_state::TeamMailboxParticipantKind::Lead => ParticipantOutput {
            kind: ParticipantKind::Lead,
            teammate_id: None,
            name: None,
        },
        codex_state::TeamMailboxParticipantKind::Teammate => ParticipantOutput {
            kind: ParticipantKind::Teammate,
            teammate_id: teammate_id.cloned(),
            name: None,
        },
    }
}

fn team_teammate_output_from_state(teammate: &codex_state::TeamTeammate) -> TeamTeammateOutput {
    TeamTeammateOutput {
        team_id: teammate.team_id.clone(),
        teammate_id: teammate.teammate_id.clone(),
        name: teammate.name.clone(),
        role: teammate.role.clone(),
        status: match teammate.status {
            codex_state::TeamTeammateStatus::Pending => TeamTeammateStatusValue::Pending,
            codex_state::TeamTeammateStatus::Active => TeamTeammateStatusValue::Active,
            codex_state::TeamTeammateStatus::Failed => TeamTeammateStatusValue::Failed,
            codex_state::TeamTeammateStatus::Closed => TeamTeammateStatusValue::Closed,
        },
        thread_id: teammate.thread_id.clone(),
        last_error: teammate.last_error.clone(),
        created_at: teammate.created_at.timestamp(),
        updated_at: teammate.updated_at.timestamp(),
    }
}

fn team_task_output_from_state(task: &codex_state::TeamTask) -> TeamTaskOutput {
    TeamTaskOutput {
        team_id: task.team_id.clone(),
        task_id: task.task_id.clone(),
        title: task.title.clone(),
        description: task.description.clone(),
        status: match task.status {
            codex_state::TeamTaskStatus::Pending => TeamTaskStatusValue::Pending,
            codex_state::TeamTaskStatus::InProgress => TeamTaskStatusValue::InProgress,
            codex_state::TeamTaskStatus::Blocked => TeamTaskStatusValue::Blocked,
            codex_state::TeamTaskStatus::Completed => TeamTaskStatusValue::Completed,
        },
        assignee_teammate_id: task.assignee_teammate_id.clone(),
        created_at: task.created_at.timestamp(),
        updated_at: task.updated_at.timestamp(),
        completed_at: task.completed_at.map(|value| value.timestamp()),
    }
}

fn team_message_output_from_state(message: &codex_state::TeamMailboxMessage) -> TeamMessageOutput {
    team_message_output_with_participants(
        message,
        participant_output_from_state(message.sender_kind, message.sender_teammate_id.as_ref()),
        participant_output_from_state(
            message.recipient_kind,
            message.recipient_teammate_id.as_ref(),
        ),
    )
}

fn team_message_output_with_participants(
    message: &codex_state::TeamMailboxMessage,
    sender: ParticipantOutput,
    recipient: ParticipantOutput,
) -> TeamMessageOutput {
    TeamMessageOutput {
        message_id: message.id.clone(),
        team_id: message.team_id.clone(),
        sender,
        recipient,
        body: message.body.clone(),
        created_at: message.created_at.timestamp(),
    }
}
