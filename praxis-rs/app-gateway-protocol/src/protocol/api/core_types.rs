use super::*;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum NonSteerableTurnKind {
    Review,
    Compact,
}

/// This translation layer exposes Praxis error codes in camel case.
///
/// When an upstream HTTP status is available (for example, from the Responses API or a provider),
/// it is forwarded in `httpStatusCode` on the relevant `praxisErrorInfo` variant.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum PraxisErrorInfo {
    ContextWindowExceeded,
    UsageLimitExceeded,
    ServerOverloaded,
    HttpConnectionFailed {
        #[serde(rename = "httpStatusCode")]
        #[ts(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    /// Failed to connect to the response SSE stream.
    ResponseStreamConnectionFailed {
        #[serde(rename = "httpStatusCode")]
        #[ts(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    InternalServerError,
    Unauthorized,
    BadRequest,
    ThreadRollbackFailed,
    SandboxError,
    /// The response SSE stream disconnected in the middle of a turn before completion.
    ResponseStreamDisconnected {
        #[serde(rename = "httpStatusCode")]
        #[ts(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    /// Reached the retry limit for responses.
    ResponseTooManyFailedAttempts {
        #[serde(rename = "httpStatusCode")]
        #[ts(rename = "httpStatusCode")]
        http_status_code: Option<u16>,
    },
    /// Returned when `turn/start` or `turn/steer` is submitted while the current active turn
    /// cannot accept same-turn steering, for example `/review` or manual `/compact`.
    ActiveTurnNotSteerable {
        #[serde(rename = "turnKind")]
        #[ts(rename = "turnKind")]
        turn_kind: NonSteerableTurnKind,
    },
    /// Returned when `turn/steer` races with completion of the expected active turn.
    NoActiveTurnToSteer,
    /// Returned when Gateway cannot resolve a thread rollout without an explicit path.
    ThreadRolloutUnavailable,
    Other,
}

impl From<CorePraxisErrorInfo> for PraxisErrorInfo {
    fn from(value: CorePraxisErrorInfo) -> Self {
        match value {
            CorePraxisErrorInfo::ContextWindowExceeded => PraxisErrorInfo::ContextWindowExceeded,
            CorePraxisErrorInfo::UsageLimitExceeded => PraxisErrorInfo::UsageLimitExceeded,
            CorePraxisErrorInfo::ServerOverloaded => PraxisErrorInfo::ServerOverloaded,
            CorePraxisErrorInfo::HttpConnectionFailed { http_status_code } => {
                PraxisErrorInfo::HttpConnectionFailed { http_status_code }
            }
            CorePraxisErrorInfo::ResponseStreamConnectionFailed { http_status_code } => {
                PraxisErrorInfo::ResponseStreamConnectionFailed { http_status_code }
            }
            CorePraxisErrorInfo::InternalServerError => PraxisErrorInfo::InternalServerError,
            CorePraxisErrorInfo::Unauthorized => PraxisErrorInfo::Unauthorized,
            CorePraxisErrorInfo::BadRequest => PraxisErrorInfo::BadRequest,
            CorePraxisErrorInfo::ThreadRollbackFailed => PraxisErrorInfo::ThreadRollbackFailed,
            CorePraxisErrorInfo::SandboxError => PraxisErrorInfo::SandboxError,
            CorePraxisErrorInfo::ResponseStreamDisconnected { http_status_code } => {
                PraxisErrorInfo::ResponseStreamDisconnected { http_status_code }
            }
            CorePraxisErrorInfo::ResponseTooManyFailedAttempts { http_status_code } => {
                PraxisErrorInfo::ResponseTooManyFailedAttempts { http_status_code }
            }
            CorePraxisErrorInfo::ActiveTurnNotSteerable { turn_kind } => {
                PraxisErrorInfo::ActiveTurnNotSteerable {
                    turn_kind: turn_kind.into(),
                }
            }
            CorePraxisErrorInfo::Other => PraxisErrorInfo::Other,
        }
    }
}

impl From<CoreNonSteerableTurnKind> for NonSteerableTurnKind {
    fn from(value: CoreNonSteerableTurnKind) -> Self {
        match value {
            CoreNonSteerableTurnKind::Review => Self::Review,
            CoreNonSteerableTurnKind::Compact => Self::Compact,
        }
    }
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS, ExperimentalApi,
)]
#[serde(rename_all = "kebab-case")]
#[ts(rename_all = "kebab-case")]
pub enum AskForApproval {
    #[serde(rename = "untrusted")]
    #[ts(rename = "untrusted")]
    UnlessTrusted,
    OnFailure,
    OnRequest,
    #[experimental("askForApproval.granular")]
    Granular {
        sandbox_approval: bool,
        rules: bool,
        #[serde(default)]
        skill_approval: bool,
        #[serde(default)]
        request_permissions: bool,
        mcp_elicitations: bool,
    },
    Never,
}

impl AskForApproval {
    pub fn to_core(self) -> CoreAskForApproval {
        match self {
            AskForApproval::UnlessTrusted => CoreAskForApproval::UnlessTrusted,
            AskForApproval::OnFailure => CoreAskForApproval::OnFailure,
            AskForApproval::OnRequest => CoreAskForApproval::OnRequest,
            AskForApproval::Granular {
                sandbox_approval,
                rules,
                skill_approval,
                request_permissions,
                mcp_elicitations,
            } => CoreAskForApproval::Granular(CoreGranularApprovalConfig {
                sandbox_approval,
                rules,
                skill_approval,
                request_permissions,
                mcp_elicitations,
            }),
            AskForApproval::Never => CoreAskForApproval::Never,
        }
    }
}

impl From<CoreAskForApproval> for AskForApproval {
    fn from(value: CoreAskForApproval) -> Self {
        match value {
            CoreAskForApproval::UnlessTrusted => AskForApproval::UnlessTrusted,
            CoreAskForApproval::OnFailure => AskForApproval::OnFailure,
            CoreAskForApproval::OnRequest => AskForApproval::OnRequest,
            CoreAskForApproval::Granular(granular_config) => AskForApproval::Granular {
                sandbox_approval: granular_config.sandbox_approval,
                rules: granular_config.rules,
                skill_approval: granular_config.skill_approval,
                request_permissions: granular_config.request_permissions,
                mcp_elicitations: granular_config.mcp_elicitations,
            },
            CoreAskForApproval::Never => AskForApproval::Never,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
/// Configures who approval requests are routed to for review. Examples
/// include sandbox escapes, blocked network access, MCP approval prompts, and
/// ARC escalations. Defaults to `user`. `guardian_subagent` uses a carefully
/// prompted subagent to gather relevant context and apply a risk-based
/// decision framework before approving or denying the request.
pub enum ApprovalsReviewer {
    User,
    GuardianSubagent,
}

impl ApprovalsReviewer {
    pub fn to_core(self) -> CoreApprovalsReviewer {
        match self {
            ApprovalsReviewer::User => CoreApprovalsReviewer::User,
            ApprovalsReviewer::GuardianSubagent => CoreApprovalsReviewer::GuardianSubagent,
        }
    }
}

impl From<CoreApprovalsReviewer> for ApprovalsReviewer {
    fn from(value: CoreApprovalsReviewer) -> Self {
        match value {
            CoreApprovalsReviewer::User => ApprovalsReviewer::User,
            CoreApprovalsReviewer::GuardianSubagent => ApprovalsReviewer::GuardianSubagent,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename_all = "kebab-case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl SandboxMode {
    pub fn to_core(self) -> CoreSandboxMode {
        match self {
            SandboxMode::ReadOnly => CoreSandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite => CoreSandboxMode::WorkspaceWrite,
            SandboxMode::DangerFullAccess => CoreSandboxMode::DangerFullAccess,
        }
    }
}

impl From<CoreSandboxMode> for SandboxMode {
    fn from(value: CoreSandboxMode) -> Self {
        match value {
            CoreSandboxMode::ReadOnly => SandboxMode::ReadOnly,
            CoreSandboxMode::WorkspaceWrite => SandboxMode::WorkspaceWrite,
            CoreSandboxMode::DangerFullAccess => SandboxMode::DangerFullAccess,
        }
    }
}

api_enum_from_core!(
    pub enum ReviewDelivery from praxis_protocol::protocol::ReviewDelivery {
        Inline, Detached
    }
);

api_enum_from_core!(
    pub enum McpAuthStatus from praxis_protocol::protocol::McpAuthStatus {
        Unsupported,
        NotLoggedIn,
        BearerToken,
        OAuth
    }
);

api_enum_from_core!(
    pub enum ModelRerouteReason from CoreModelRerouteReason {
        HighRiskCyberActivity
    }
);

api_enum_from_core!(
    pub enum HookEventName from CoreHookEventName {
        PreToolUse, PostToolUse, SessionStart, UserPromptSubmit, Stop
    }
);

api_enum_from_core!(
    pub enum HookHandlerType from CoreHookHandlerType {
        Command, Prompt, Agent
    }
);

api_enum_from_core!(
    pub enum HookExecutionMode from CoreHookExecutionMode {
        Sync, Async
    }
);

api_enum_from_core!(
    pub enum HookScope from CoreHookScope {
        Thread, Turn
    }
);

api_enum_from_core!(
    pub enum HookRunStatus from CoreHookRunStatus {
        Running, Completed, Failed, Blocked, Stopped
    }
);

api_enum_from_core!(
    pub enum HookOutputEntryKind from CoreHookOutputEntryKind {
        Warning, Stop, Feedback, Context, Error
    }
);

api_enum_from_core!(
    pub enum ThreadGoalStatus from CoreThreadGoalStatus {
        Active, Paused, Blocked, UsageLimited, BudgetLimited, Complete
    }
);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct HookOutputEntry {
    pub kind: HookOutputEntryKind,
    pub text: String,
}

impl From<CoreHookOutputEntry> for HookOutputEntry {
    fn from(value: CoreHookOutputEntry) -> Self {
        Self {
            kind: value.kind.into(),
            text: value.text,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct HookRunSummary {
    pub id: String,
    pub event_name: HookEventName,
    pub handler_type: HookHandlerType,
    pub execution_mode: HookExecutionMode,
    pub scope: HookScope,
    pub source_path: PathBuf,
    pub display_order: i64,
    pub status: HookRunStatus,
    pub status_message: Option<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub entries: Vec<HookOutputEntry>,
}

impl From<CoreHookRunSummary> for HookRunSummary {
    fn from(value: CoreHookRunSummary) -> Self {
        Self {
            id: value.id,
            event_name: value.event_name.into(),
            handler_type: value.handler_type.into(),
            execution_mode: value.execution_mode.into(),
            scope: value.scope.into(),
            source_path: value.source_path,
            display_order: value.display_order,
            status: value.status.into(),
            status_message: value.status_message,
            started_at: value.started_at,
            completed_at: value.completed_at,
            duration_ms: value.duration_ms,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}
