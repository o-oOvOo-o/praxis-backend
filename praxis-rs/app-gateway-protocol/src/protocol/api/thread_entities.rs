use super::*;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    /// Usually the first user message in the thread, if available.
    pub preview: String,
    /// Optional persisted session summary for picker/list surfaces.
    pub summary: Option<String>,
    /// Whether the thread is ephemeral and should not be materialized on disk.
    pub ephemeral: bool,
    /// Model provider used for this thread (for example, 'openai').
    pub model_provider: String,
    /// Latest observed model used for this thread, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    /// Unix timestamp (in seconds) when the thread was created.
    #[ts(type = "number")]
    pub created_at: i64,
    /// Unix timestamp (in seconds) when the thread was last updated.
    #[ts(type = "number")]
    pub updated_at: i64,
    /// Current runtime status for the thread.
    pub status: ThreadStatus,
    /// [UNSTABLE] Path to the thread on disk.
    pub path: Option<PathBuf>,
    /// Working directory captured for the thread.
    pub cwd: PathBuf,
    /// Version of the CLI that created the thread.
    pub cli_version: String,
    /// Origin of the thread (CLI, VSCode, praxis exec, praxis app-gateway, etc.).
    pub source: SessionSource,
    /// Optional base name assigned to an AgentControl-spawned sub-agent.
    pub agent_base_name: Option<String>,
    /// Optional short responsibility title assigned to an AgentControl-spawned sub-agent.
    pub agent_title: Option<String>,
    /// Optional display name assigned to an AgentControl-spawned sub-agent.
    pub agent_display_name: Option<String>,
    /// Optional role (agent_role) assigned to an AgentControl-spawned sub-agent.
    pub agent_role: Option<String>,
    /// Optional Git metadata captured when the thread was created.
    pub git_info: Option<GitInfo>,
    /// Optional user-facing thread title.
    pub name: Option<String>,
    /// Optional estimated total thread cost in USD.
    pub total_cost_usd: Option<f64>,
    /// Optional estimated last-turn cost in USD.
    pub last_cost_usd: Option<f64>,
    /// Optional persisted token usage snapshot for list/detail surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub token_usage: Option<ThreadTokenUsage>,
    /// Optional live controller lock for read-only observation surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub control_state: Option<ThreadControlState>,
    /// Optional persisted markdown plan path for idle-driven autonomous selfwork.
    pub selfwork_plan_path: Option<PathBuf>,
    /// Only populated on `thread/resume`, `thread/rollback`, `thread/fork`, and `thread/read`
    /// (when `includeTurns` is true) responses.
    /// For all other responses and notifications returning a Thread,
    /// the turns field will be an empty list.
    pub turns: Vec<Turn>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountUpdatedNotification {
    pub auth_mode: Option<AuthMode>,
    pub plan_type: Option<PlanType>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsageUpdatedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub token_usage: ThreadTokenUsage,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ThreadControllerKind {
    Thread,
    External,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadController {
    pub kind: ThreadControllerKind,
    /// Thread id for thread controllers or stable client id for external controllers.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// Agent group rank 0/1 controllers may acquire locks. External controllers are trusted by the gateway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub rank: Option<u8>,
}

impl ThreadController {
    pub fn validate_control_access(&self, target_rank: Option<u8>) -> Result<(), String> {
        match self.kind {
            ThreadControllerKind::External => Ok(()),
            ThreadControllerKind::Thread => {
                let Some(rank) = self.rank else {
                    return Err(
                        "agent group thread controllers must include rank 0 or rank 1".to_string(),
                    );
                };
                if rank > 1 {
                    return Err(
                        "only agent group rank 0 and rank 1 threads can control other threads"
                            .to_string(),
                    );
                }
                if let Some(target_rank) = target_rank
                    && rank >= target_rank
                {
                    return Err(format!(
                        "agent group rank {rank} cannot control rank {target_rank}; same-rank and higher-rank control is forbidden"
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlState {
    pub controller: ThreadController,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// True while local clients should keep the thread transcript read-only.
    pub read_only: bool,
    #[ts(type = "number")]
    pub acquired_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlClaimParams {
    pub thread_id: String,
    pub controller: ThreadController,
    /// Optional target agent group rank, used when clients already know it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub target_rank: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlClaimResponse {
    pub control_state: ThreadControlState,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ThreadControlQueueStatus {
    Queued,
    Dispatched,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueItem {
    pub queue_id: String,
    pub target_thread_id: String,
    pub controller: ThreadController,
    pub text: String,
    pub status: ThreadControlQueueStatus,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub dispatched_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlSnapshotParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlSnapshotResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub control_state: Option<ThreadControlState>,
    pub queue: Vec<ThreadControlQueueItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueParams {
    pub thread_id: String,
    pub controller: ThreadController,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueResponse {
    pub item: ThreadControlQueueItem,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueCancelParams {
    pub thread_id: String,
    pub queue_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueCancelResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub item: Option<ThreadControlQueueItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueFlushParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlQueueFlushResponse {
    pub cancelled: Vec<ThreadControlQueueItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlReleaseParams {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub controller: Option<ThreadController>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlReleaseResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub previous_control_state: Option<ThreadControlState>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadControlChangedNotification {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub control_state: Option<ThreadControlState>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoal {
    pub thread_id: String,
    pub objective: String,
    pub status: ThreadGoalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token_budget: Option<i64>,
    pub tokens_used: i64,
    pub time_used_seconds: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<CoreThreadGoal> for ThreadGoal {
    fn from(value: CoreThreadGoal) -> Self {
        Self {
            thread_id: value.thread_id.to_string(),
            objective: value.objective,
            status: value.status.into(),
            token_budget: value.token_budget,
            tokens_used: value.tokens_used,
            time_used_seconds: value.time_used_seconds,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalGetParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalGetResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub goal: Option<ThreadGoal>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalSetParams {
    pub thread_id: String,
    pub objective: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token_budget: Option<i64>,
    #[serde(default)]
    pub clear_token_budget: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalSetResponse {
    pub goal: ThreadGoal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalUpdateParams {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub objective: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<ThreadGoalStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token_budget: Option<i64>,
    #[serde(default)]
    pub clear_token_budget: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalUpdateResponse {
    pub goal: ThreadGoal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalClearParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalClearResponse {
    pub cleared: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalUpdatedNotification {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    pub goal: ThreadGoal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoalClearedNotification {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeat {
    pub thread_id: String,
    pub enabled: bool,
    pub interval_ms: i64,
    pub next_wake_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_wake_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub controller: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl From<CoreThreadHeartbeat> for ThreadHeartbeat {
    fn from(value: CoreThreadHeartbeat) -> Self {
        Self {
            thread_id: value.thread_id.to_string(),
            enabled: value.enabled,
            interval_ms: value.interval_ms,
            next_wake_at_ms: value.next_wake_at_ms,
            last_wake_at_ms: value.last_wake_at_ms,
            controller: value.controller,
            created_at_ms: value.created_at_ms,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatGetParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatGetResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heartbeat: Option<ThreadHeartbeat>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatSetParams {
    pub thread_id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interval_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub controller: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatSetResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heartbeat: Option<ThreadHeartbeat>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatClearParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatClearResponse {
    pub cleared: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeartbeatUpdatedNotification {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heartbeat: Option<ThreadHeartbeat>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsage {
    pub total: TokenUsageBreakdown,
    pub last: TokenUsageBreakdown,
    // TODO(aibrahim): make this not optional
    #[ts(type = "number | null")]
    pub model_context_window: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub model_auto_compact_token_limit: Option<i64>,
}

impl From<CoreTokenUsageInfo> for ThreadTokenUsage {
    fn from(value: CoreTokenUsageInfo) -> Self {
        Self {
            total: value.total_token_usage.into(),
            last: value.last_token_usage.into(),
            model_context_window: value.model_context_window,
            model_auto_compact_token_limit: value.model_auto_compact_token_limit,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBreakdown {
    #[serde(default)]
    #[ts(type = "number")]
    pub total_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub input_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub cached_input_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub cache_reported_input_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub output_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub reasoning_output_tokens: i64,
}

impl From<CoreTokenUsage> for TokenUsageBreakdown {
    fn from(value: CoreTokenUsage) -> Self {
        Self {
            total_tokens: value.total_tokens,
            input_tokens: value.input_tokens,
            cached_input_tokens: value.cached_input_tokens,
            cache_reported_input_tokens: value.cache_reported_input_tokens,
            output_tokens: value.output_tokens,
            reasoning_output_tokens: value.reasoning_output_tokens,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    /// Only populated on a `thread/resume` or `thread/fork` response.
    /// For all other responses and notifications returning a Turn,
    /// the items field will be an empty list.
    pub items: Vec<ThreadItem>,
    pub status: TurnStatus,
    /// Only populated when the Turn's status is failed.
    pub error: Option<TurnError>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCitation {
    pub entries: Vec<MemoryCitationEntry>,
    pub thread_ids: Vec<String>,
}

impl From<CoreMemoryCitation> for MemoryCitation {
    fn from(value: CoreMemoryCitation) -> Self {
        Self {
            entries: value.entries.into_iter().map(Into::into).collect(),
            thread_ids: value.rollout_ids,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCitationEntry {
    pub path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub note: String,
}

impl From<CoreMemoryCitationEntry> for MemoryCitationEntry {
    fn from(value: CoreMemoryCitationEntry) -> Self {
        Self {
            path: value.path,
            line_start: value.line_start,
            line_end: value.line_end,
            note: value.note,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS, Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct TurnError {
    pub message: String,
    pub praxis_error_info: Option<PraxisErrorInfo>,
    #[serde(default)]
    pub additional_details: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorNotification {
    pub error: TurnError,
    // Set to true if the error is transient and the app-gateway process will automatically retry.
    // If true, this will not interrupt a turn.
    pub will_retry: bool,
    pub thread_id: String,
    pub turn_id: String,
}

/// EXPERIMENTAL - thread realtime audio chunk.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRealtimeAudioChunk {
    pub data: String,
    pub sample_rate: u32,
    pub num_channels: u16,
    pub samples_per_channel: Option<u32>,
    pub item_id: Option<String>,
}

impl From<CoreRealtimeAudioFrame> for ThreadRealtimeAudioChunk {
    fn from(value: CoreRealtimeAudioFrame) -> Self {
        let CoreRealtimeAudioFrame {
            data,
            sample_rate,
            num_channels,
            samples_per_channel,
            item_id,
        } = value;
        Self {
            data,
            sample_rate,
            num_channels,
            samples_per_channel,
            item_id,
        }
    }
}

impl From<ThreadRealtimeAudioChunk> for CoreRealtimeAudioFrame {
    fn from(value: ThreadRealtimeAudioChunk) -> Self {
        let ThreadRealtimeAudioChunk {
            data,
            sample_rate,
            num_channels,
            samples_per_channel,
            item_id,
        } = value;
        Self {
            data,
            sample_rate,
            num_channels,
            samples_per_channel,
            item_id,
        }
    }
}
