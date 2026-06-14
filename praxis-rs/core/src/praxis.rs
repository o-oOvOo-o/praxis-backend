use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;

use crate::WireApi;
use crate::agent::AgentControl;
use crate::agent::AgentStatus;
use crate::agent::Mailbox;
use crate::agent::MailboxReceiver;
use crate::agent::agent_status_from_event;
use crate::agent::status::is_final;
use crate::apps::render_apps_section;
use crate::auto_title_profile::AutoTitleProfile;
use crate::auto_title_profile::select_auto_title_model;
use crate::commit_attribution::commit_message_trailer_instruction;
use crate::compact;
use crate::compact::InitialContextInjection;
use crate::compact::run_inline_auto_compact_task;
use crate::compact::should_use_remote_compact_task;
use crate::compact_remote::run_inline_remote_auto_compact_task;
use crate::config::ManagedFeatures;
use crate::connectors;
use crate::contextual_user_message::RUNTIME_RECOVERY_FRAGMENT;
use crate::exec_policy::ExecPolicyManager;
use crate::llm::prompts::LlmPromptPurpose;
use crate::llm::runtime::LlmRuntimeCatalog;
#[cfg(test)]
use crate::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::manager::RefreshStrategy;
use crate::parse_turn_item;
use crate::path_utils::normalize_for_native_workdir;
use crate::provider_decision_center::ProviderDecisionCenter;
use crate::realtime_conversation::RealtimeConversationManager;
use crate::realtime_conversation::handle_audio as handle_realtime_conversation_audio;
use crate::realtime_conversation::handle_close as handle_realtime_conversation_close;
use crate::realtime_conversation::handle_start as handle_realtime_conversation_start;
use crate::realtime_conversation::handle_text as handle_realtime_conversation_text;
use crate::render_skills_section;
use crate::session_prefix::format_subagent_notification_message;
use crate::skills_load_input_from_config;
use crate::stream_events_utils::HandleOutputCtx;
use crate::stream_events_utils::emit_synthetic_final_answer;
use crate::stream_events_utils::handle_non_tool_response_item;
use crate::stream_events_utils::handle_output_item_done;
use crate::stream_events_utils::last_assistant_message_from_item;
use crate::stream_events_utils::raw_assistant_output_text_from_item;
use crate::stream_events_utils::synthetic_final_item_for_guard;
use crate::tools::loop_guard::ToolLoopGuardState;
use crate::turn_metadata::TurnMetadataState;
use crate::util::error_or_panic;
use async_channel::Receiver;
use async_channel::Sender;
use chrono::Local;
use chrono::Utc;
use futures::future::BoxFuture;
use futures::future::Shared;
use futures::prelude::*;
use futures::stream::FuturesOrdered;
use praxis_analytics::AnalyticsEventsClient;
use praxis_analytics::AppInvocation;
use praxis_analytics::InvocationType;
use praxis_analytics::build_track_events_context;
use praxis_exec_server::Environment;
use praxis_exec_server::EnvironmentManager;
use praxis_features::FEATURES;
use praxis_features::Feature;
use praxis_features::unstable_features_warning_event;
use praxis_hooks::HookEvent;
use praxis_hooks::HookEventAfterAgent;
use praxis_hooks::HookPayload;
use praxis_hooks::HookResult;
use praxis_hooks::Hooks;
use praxis_hooks::HooksConfig;
use praxis_login::AuthManager;
use praxis_login::CodexAuth;
use praxis_login::default_client::originator;
use praxis_mcp::mcp_connection_manager::McpConnectionManager;
use praxis_mcp::mcp_connection_manager::SandboxState;
use praxis_mcp::mcp_connection_manager::ToolInfo as McpToolInfo;
use praxis_mcp::mcp_connection_manager::filter_non_praxis_apps_mcp_tools_only;
use praxis_mcp::mcp_connection_manager::praxis_apps_tools_cache_key;
use praxis_network_proxy::NetworkProxy;
use praxis_network_proxy::NetworkProxyAuditMetadata;
use praxis_network_proxy::normalize_host;
use praxis_otel::current_span_trace_id;
use praxis_otel::current_span_w3c_trace_context;
use praxis_otel::set_parent_from_w3c_trace_context;
use praxis_protocol::ThreadId;
use praxis_protocol::approvals::ElicitationRequestEvent;
use praxis_protocol::approvals::ExecPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyRuleAction;
use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::dynamic_tools::DynamicToolResponse;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::items::PlanItem;
use praxis_protocol::items::TurnItem;
use praxis_protocol::items::UserMessageItem;
use praxis_protocol::items::build_hook_prompt_message;
use praxis_protocol::mcp::CallToolResult;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequest;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequestParams;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::models::format_allow_prefixes;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_protocol::protocol::FileChange;
use praxis_protocol::protocol::HasLegacyEvent;
use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::ItemCompletedEvent;
use praxis_protocol::protocol::ItemStartedEvent;
use praxis_protocol::protocol::RawResponseItemEvent;
use praxis_protocol::protocol::ReviewRequest;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnContextItem;
use praxis_protocol::protocol::TurnContextNetworkItem;
use praxis_protocol::protocol::W3cTraceContext;
use praxis_protocol::request_permissions::PermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsArgs;
use praxis_protocol::request_permissions::RequestPermissionsEvent;
use praxis_protocol::request_permissions::RequestPermissionsResponse;
use praxis_protocol::request_user_input::RequestUserInputArgs;
use praxis_protocol::request_user_input::RequestUserInputResponse;
use praxis_rmcp_client::ElicitationResponse;
use praxis_rmcp_client::OAuthCredentialsStoreMode;
use praxis_rollout::state_db;
use praxis_shell_command::parse_command::parse_command;
use praxis_tools::filter_tool_suggest_discoverable_tools_for_client;
use praxis_utils_output_truncation::TruncationPolicy;
use praxis_utils_output_truncation::truncate_text;
use praxis_utils_stream_parser::AssistantTextChunk;
use praxis_utils_stream_parser::AssistantTextStreamParser;
use praxis_utils_stream_parser::ProposedPlanSegment;
use praxis_utils_stream_parser::extract_proposed_plan_text;
use praxis_utils_stream_parser::strip_citations;
use rmcp::model::ListResourceTemplatesResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::PaginatedRequestParams;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::ReadResourceResult;
use rmcp::model::RequestId;
use serde_json;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use toml::Value as TomlValue;
use tracing::Instrument;
use tracing::debug;
use tracing::debug_span;
use tracing::error;
use tracing::field;
use tracing::info;
use tracing::info_span;
use tracing::instrument;
use tracing::trace;
use tracing::trace_span;
use tracing::warn;
use uuid::Uuid;

use crate::ModelProviderInfo;
use crate::client::ModelClientSession;
use crate::client::ModelRuntimeRegistry;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::compact::collect_user_messages;
use crate::config::Config;
use crate::config::Constrained;
use crate::config::ConstraintError;
use crate::config::ConstraintResult;
use crate::config::GhostSnapshotConfig;
use crate::config::StartedNetworkProxy;
use crate::config::resolve_web_search_mode_for_turn;
use crate::config_loader::RequirementSource;
use crate::context_manager::ContextManager;
use crate::context_manager::TotalTokenUsageBreakdown;
use crate::environment_context::EnvironmentContext;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
#[cfg(test)]
use crate::exec::StreamOutput;
use crate::praxis_thread::ThreadConfigSnapshot;
use praxis_config::CONFIG_TOML_FILE;
use praxis_config::types::McpServerConfig;
use praxis_config::types::ShellEnvironmentPolicy;

mod rollout_reconstruction;
#[cfg(test)]
mod rollout_reconstruction_tests;
mod sampling;
mod session_io;
mod turn_interactions;

const PENDING_INPUT_CHECK_TIMEOUT_MS: u64 = 2_000;

#[derive(Debug, PartialEq)]
pub enum SteerInputError {
    NoActiveTurn(Vec<UserInput>),
    ExpectedTurnMismatch { expected: String, actual: String },
    ActiveTurnNotSteerable { turn_kind: NonSteerableTurnKind },
    EmptyInput,
}

impl SteerInputError {
    fn to_error_event(&self) -> ErrorEvent {
        match self {
            Self::NoActiveTurn(_) => ErrorEvent {
                message: "no active turn to steer".to_string(),
                praxis_error_info: Some(CodexErrorInfo::BadRequest),
            },
            Self::ExpectedTurnMismatch { expected, actual } => ErrorEvent {
                message: format!("expected active turn id `{expected}` but found `{actual}`"),
                praxis_error_info: Some(CodexErrorInfo::BadRequest),
            },
            Self::ActiveTurnNotSteerable { turn_kind } => {
                let turn_kind_label = match turn_kind {
                    NonSteerableTurnKind::Review => "review",
                    NonSteerableTurnKind::Compact => "compact",
                };
                ErrorEvent {
                    message: format!("cannot steer a {turn_kind_label} turn"),
                    praxis_error_info: Some(CodexErrorInfo::ActiveTurnNotSteerable {
                        turn_kind: *turn_kind,
                    }),
                }
            }
            Self::EmptyInput => ErrorEvent {
                message: "input must not be empty".to_string(),
                praxis_error_info: Some(CodexErrorInfo::BadRequest),
            },
        }
    }
}

/// Notes from the previous real user turn.
///
/// Conceptually this is the same role that `previous_model` used to fill, but
/// it can carry other prior-turn settings that matter when constructing
/// sensible state-change diffs or full-context reinjection, such as model
/// switches or detecting a prior `realtime_active -> false` transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviousTurnSettings {
    pub(crate) model: String,
    pub(crate) realtime_active: Option<bool>,
}

use crate::SkillError;
use crate::SkillInjections;
use crate::SkillLoadOutcome;
use crate::SkillMetadata;
use crate::SkillsManager;
use crate::agent_os::AgentOs;
use crate::agent_os::RuntimeCommandRecord;
use crate::agent_os::ThreadRegistration;
use crate::agent_os::coordination_scope_for_session_source;
use crate::agent_os::profile_for_rank;
use crate::agent_os::rank_for_session_source;
use crate::build_skill_injections;
use crate::collect_env_var_dependencies;
use crate::collect_explicit_skill_mentions;
use crate::exec_policy::ExecPolicyUpdateError;
use crate::feedback_tags;
use crate::guardian::GuardianReviewSessionManager;
use crate::hook_runtime::process_pending_input_for_sampling;
use crate::hook_runtime::record_additional_contexts;
use crate::hook_runtime::run_pending_session_start_hooks;
use crate::hook_runtime::run_user_prompt_submit_hooks;
use crate::injection::ToolMentionKind;
use crate::injection::app_id_from_path;
use crate::injection::tool_kind_for_path;
use crate::instructions::UserInstructions;
use crate::mcp::McpManager;
use crate::mcp_skill_dependencies::maybe_prompt_and_install_mcp_dependencies;
use crate::memories;
use crate::mentions::build_connector_slug_counts;
use crate::mentions::build_skill_name_counts;
use crate::mentions::collect_explicit_app_ids;
use crate::mentions::collect_explicit_plugin_mentions;
use crate::mentions::collect_tool_mentions_from_messages;
use crate::network_policy_decision::execpolicy_network_rule_amendment;
use crate::plugins::PluginsManager;
use crate::plugins::build_plugin_injections;
use crate::plugins::render_plugins_section;
use crate::project_doc::get_user_instructions;
use crate::resolve_skill_dependencies_for_turn;
use crate::rollout::RolloutRecorder;
use crate::rollout::RolloutRecorderParams;
use crate::rollout::map_session_init_error;
use crate::rollout::metadata;
use crate::rollout::policy::EventPersistenceMode;
use crate::session_startup_prewarm::SessionStartupPrewarmHandle;
use crate::shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::skills_watcher::SkillsWatcher;
use crate::skills_watcher::SkillsWatcherEvent;
use crate::state::ActiveTurn;
use crate::state::SessionServices;
use crate::state::SessionState;
use crate::tasks::AgentTask;
use crate::tasks::AgentTaskContext;
use crate::tasks::GhostSnapshotTask;
use crate::tasks::ReviewTask;
use crate::tools::ToolRouter;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::network_approval::NetworkApprovalService;
use crate::tools::network_approval::build_blocked_request_observer;
use crate::tools::network_approval::build_network_policy_decider;
use crate::tools::router::ToolRouterParams;
use crate::tools::runtimes::shell::ShellHostProcessCleaner;
use crate::tools::sandboxing::ApprovalStore;
use crate::tools::tool_call_runtime::ToolCallRuntime;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::turn_timing::TurnTimingState;
use crate::turn_timing::record_turn_ttfm_metric;
use crate::turn_timing::record_turn_ttft_metric;
use crate::unified_exec::UnifiedExecProcessManager;
use crate::util::backoff;
use crate::windows_sandbox::WindowsSandboxLevelExt;
use praxis_async_utils::OrCancelExt;
use praxis_git_utils::get_git_repo_root;
use praxis_mcp::mcp::CODEX_APPS_MCP_SERVER_NAME;
use praxis_mcp::mcp::auth::compute_auth_statuses;
use praxis_mcp::mcp::with_praxis_apps_mcp;
use praxis_otel::SessionTelemetry;
use praxis_otel::TelemetryAuthMode;
use praxis_otel::metrics::names::THREAD_STARTED_METRIC;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::DeveloperInstructions;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
use praxis_protocol::protocol::AgentReasoningSectionBreakEvent;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::BackgroundEventEvent;
use praxis_protocol::protocol::CodexErrorInfo;
use praxis_protocol::protocol::CompactedItem;
use praxis_protocol::protocol::DeprecationNoticeEvent;
use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::McpServerRefreshConfig;
use praxis_protocol::protocol::ModelRerouteEvent;
use praxis_protocol::protocol::ModelRerouteReason;
use praxis_protocol::protocol::NetworkApprovalContext;
use praxis_protocol::protocol::NonSteerableTurnKind;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::PlanDeltaEvent;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::ReasoningContentDeltaEvent;
use praxis_protocol::protocol::ReasoningRawContentDeltaEvent;
use praxis_protocol::protocol::RequestUserInputEvent;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;
use praxis_protocol::protocol::SkillDependencies as ProtocolSkillDependencies;
use praxis_protocol::protocol::SkillErrorInfo;
use praxis_protocol::protocol::SkillInterface as ProtocolSkillInterface;
use praxis_protocol::protocol::SkillMetadata as ProtocolSkillMetadata;
use praxis_protocol::protocol::SkillToolDependency as ProtocolSkillToolDependency;
use praxis_protocol::protocol::StreamErrorEvent;
use praxis_protocol::protocol::Submission;
use praxis_protocol::protocol::ThreadNameUpdatedEvent;
use praxis_protocol::protocol::TokenCountEvent;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;
use praxis_protocol::protocol::TurnDiffEvent;
use praxis_protocol::protocol::WarningEvent;
use praxis_protocol::user_input::UserInput;
use praxis_tools::ToolCapabilityConfig;
use praxis_tools::ToolWireProfile;
use praxis_tools::ToolsConfig;
use praxis_tools::ToolsConfigParams;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_readiness::Readiness;
use praxis_utils_readiness::ReadinessFlag;

/// The high-level interface to the Praxis system.
/// It operates as a queue pair where you send submissions and receive events.
pub struct Praxis {
    pub(crate) tx_sub: Sender<Submission>,
    pub(crate) rx_event: Receiver<Event>,
    // Last known status of the agent.
    pub(crate) agent_status: watch::Receiver<AgentStatus>,
    pub(crate) session: Arc<Session>,
    // Shared future for the background submission loop completion so multiple
    // callers can wait for shutdown.
    pub(crate) session_loop_termination: SessionLoopTermination,
}

pub(crate) type SessionLoopTermination = Shared<BoxFuture<'static, ()>>;

fn tool_wire_profile_for_wire_api(wire_api: WireApi) -> ToolWireProfile {
    match wire_api {
        WireApi::Responses => ToolWireProfile::Responses,
        WireApi::Claude => ToolWireProfile::Claude,
        WireApi::OpenAiCompat => ToolWireProfile::Common,
    }
}

fn tool_capabilities_for_turn_model(
    llm_runtime_catalog: &LlmRuntimeCatalog,
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
    session_source: &SessionSource,
) -> ToolCapabilityConfig {
    llm_runtime_catalog.tool_capabilities_for_model(
        model_info,
        provider_id,
        provider,
        session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product),
    )
}

/// Wrapper returned by [`Praxis::spawn`] containing the spawned [`Praxis`],
/// the submission id for the initial `ConfigureSession` request and the
/// unique session id.
pub struct PraxisSpawnOk {
    pub praxis: Praxis,
    pub thread_id: ThreadId,
}

pub(crate) struct PraxisSpawnArgs {
    pub(crate) config: Config,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) models_manager: Arc<ModelsManager>,
    pub(crate) environment_manager: Arc<EnvironmentManager>,
    pub(crate) skills_manager: Arc<SkillsManager>,
    pub(crate) plugins_manager: Arc<PluginsManager>,
    pub(crate) mcp_manager: Arc<McpManager>,
    pub(crate) skills_watcher: Arc<SkillsWatcher>,
    pub(crate) conversation_history: InitialHistory,
    pub(crate) session_source: SessionSource,
    pub(crate) agent_control: AgentControl,
    pub(crate) agent_os: Arc<AgentOs>,
    pub(crate) dynamic_tools: Vec<DynamicToolSpec>,
    pub(crate) persist_extended_history: bool,
    pub(crate) metrics_service_name: Option<String>,
    pub(crate) inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    pub(crate) inherited_exec_policy: Option<Arc<ExecPolicyManager>>,
    pub(crate) user_shell_override: Option<shell::Shell>,
    pub(crate) parent_trace: Option<W3cTraceContext>,
}

pub(crate) const INITIAL_SUBMIT_ID: &str = "";
pub(crate) const SUBMISSION_CHANNEL_CAPACITY: usize = 512;
const CYBER_VERIFY_URL: &str = "https://chatgpt.com/cyber";
const CYBER_SAFETY_URL: &str = "https://developers.openai.com/codex/concepts/cyber-safety";
const DIRECT_APP_TOOL_EXPOSURE_THRESHOLD: usize = 100;

fn merge_plugin_model_catalog(config: &mut Config, llm_runtime_catalog: &LlmRuntimeCatalog) {
    let plugin_models = llm_runtime_catalog
        .model_infos_for_provider(&config.model_provider_id, &config.model_provider);
    if plugin_models.is_empty() {
        return;
    }

    let model_catalog = config
        .model_catalog
        .get_or_insert_with(ModelsResponse::default);
    for plugin_model in plugin_models {
        if let Some(existing) = model_catalog
            .models
            .iter_mut()
            .find(|model| model.slug == plugin_model.slug)
        {
            *existing = plugin_model;
        } else {
            model_catalog.models.push(plugin_model);
        }
    }
    model_catalog
        .models
        .sort_by(|left, right| left.priority.cmp(&right.priority));
}

#[cfg(test)]
pub(crate) fn completed_session_loop_termination() -> SessionLoopTermination {
    futures::future::ready(()).boxed().shared()
}

pub(crate) fn session_loop_termination_from_handle(
    handle: JoinHandle<()>,
) -> SessionLoopTermination {
    async move {
        let _ = handle.await;
    }
    .boxed()
    .shared()
}

/// Context for an initialized model agent
///
/// A session has at most 1 running task at a time, and can be interrupted by user input.
pub(crate) struct Session {
    pub(crate) conversation_id: ThreadId,
    tx_event: Sender<Event>,
    agent_status: watch::Sender<AgentStatus>,
    out_of_band_elicitation_paused: watch::Sender<bool>,
    state: Mutex<SessionState>,
    /// The set of enabled features should be invariant for the lifetime of the
    /// session.
    features: ManagedFeatures,
    pending_mcp_server_refresh_config: Mutex<Option<McpServerRefreshConfig>>,
    pub(crate) conversation: Arc<RealtimeConversationManager>,
    pub(crate) active_turn: Mutex<Option<ActiveTurn>>,
    mailbox: Mailbox,
    mailbox_rx: Mutex<MailboxReceiver>,
    idle_pending_input: Mutex<Vec<ResponseInputItem>>, // TODO (jif) merge with mailbox!
    pub(crate) guardian_review_session: GuardianReviewSessionManager,
    pub(crate) services: SessionServices,
    pub(crate) goal_runtime: crate::goals::GoalRuntimeState,
    llm_runtime_catalog: LlmRuntimeCatalog,
    next_internal_sub_id: AtomicU64,
    /// Guards one-shot auto-title generation so it runs at most once per session.
    pub(crate) auto_title_attempted: AtomicBool,
    /// Avoids overlapping auto-summary generations for the same thread.
    pub(crate) auto_summary_in_flight: AtomicBool,
}

#[derive(Clone, Debug)]
pub(crate) struct TurnSkillsContext {
    pub(crate) outcome: Arc<SkillLoadOutcome>,
    pub(crate) implicit_invocation_seen_skills: Arc<Mutex<HashSet<String>>>,
}

impl TurnSkillsContext {
    pub(crate) fn new(outcome: Arc<SkillLoadOutcome>) -> Self {
        Self {
            outcome,
            implicit_invocation_seen_skills: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

/// The context needed for a single turn of the thread.
#[derive(Debug)]
pub(crate) struct TurnContext {
    pub(crate) sub_id: String,
    pub(crate) trace_id: Option<String>,
    pub(crate) realtime_active: bool,
    pub(crate) config: Arc<Config>,
    pub(crate) auth_manager: Option<Arc<AuthManager>>,
    pub(crate) model_info: ModelInfo,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) reasoning_effort: Option<ReasoningEffortConfig>,
    pub(crate) reasoning_summary: ReasoningSummaryConfig,
    pub(crate) session_source: SessionSource,
    pub(crate) environment: Arc<Environment>,
    /// The session's absolute working directory. All relative paths provided
    /// by the model as well as sandbox policies are resolved against this path
    /// instead of `std::env::current_dir()`.
    pub(crate) cwd: AbsolutePathBuf,
    pub(crate) current_date: Option<String>,
    pub(crate) timezone: Option<String>,
    pub(crate) app_gateway_client_name: Option<String>,
    pub(crate) developer_instructions: Option<String>,
    pub(crate) compact_prompt: Option<String>,
    pub(crate) user_instructions: Option<String>,
    pub(crate) collaboration_mode: CollaborationMode,
    pub(crate) personality: Option<Personality>,
    pub(crate) approval_policy: Constrained<AskForApproval>,
    pub(crate) sandbox_policy: Constrained<SandboxPolicy>,
    pub(crate) file_system_sandbox_policy: FileSystemSandboxPolicy,
    pub(crate) network_sandbox_policy: NetworkSandboxPolicy,
    pub(crate) network: Option<NetworkProxy>,
    pub(crate) windows_sandbox_level: WindowsSandboxLevel,
    pub(crate) shell_environment_policy: ShellEnvironmentPolicy,
    pub(crate) tools_config: ToolsConfig,
    pub(crate) features: ManagedFeatures,
    pub(crate) ghost_snapshot: GhostSnapshotConfig,
    pub(crate) final_output_json_schema: Option<Value>,
    pub(crate) praxis_self_exe: Option<PathBuf>,
    pub(crate) praxis_linux_sandbox_exe: Option<PathBuf>,
    pub(crate) tool_call_gate: Arc<ReadinessFlag>,
    pub(crate) tool_loop_guard: Arc<ToolLoopGuardState>,
    pub(crate) truncation_policy: TruncationPolicy,
    pub(crate) dynamic_tools: Vec<DynamicToolSpec>,
    pub(crate) turn_metadata_state: Arc<TurnMetadataState>,
    pub(crate) turn_skills: TurnSkillsContext,
    pub(crate) turn_timing_state: Arc<TurnTimingState>,
}

pub(crate) struct AutoTitleModelContext {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model_info: ModelInfo,
    pub(crate) instructions: Option<String>,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) service_tier: Option<ServiceTier>,
    pub(crate) personality: Option<Personality>,
    pub(crate) profile: AutoTitleProfile,
    pub(crate) reasoning_effort: Option<ReasoningEffortConfig>,
}

pub(crate) struct AutoSummaryModelContext {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
    pub(crate) model_info: ModelInfo,
    pub(crate) instructions: Option<String>,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) service_tier: Option<ServiceTier>,
    pub(crate) personality: Option<Personality>,
}

impl TurnContext {
    pub(crate) fn model_context_window(&self) -> Option<i64> {
        let effective_context_window_percent = self.model_info.effective_context_window_percent;
        self.model_info.context_window.map(|context_window| {
            context_window.saturating_mul(effective_context_window_percent) / 100
        })
    }

    pub(crate) fn apps_enabled(&self) -> bool {
        self.features
            .apps_enabled_cached(self.auth_manager.as_deref())
    }

    pub(crate) async fn with_model(&self, model: String, models_manager: &ModelsManager) -> Self {
        let mut config = (*self.config).clone();
        config.model = Some(model.clone());
        let model_info = models_manager.get_model_info(model.as_str(), &config).await;
        let truncation_policy = model_info.truncation_policy.into();
        let supported_reasoning_levels = model_info
            .supported_reasoning_levels
            .iter()
            .map(|preset| preset.effort)
            .collect::<Vec<_>>();
        let reasoning_effort = if let Some(current_reasoning_effort) = self.reasoning_effort {
            if supported_reasoning_levels.contains(&current_reasoning_effort) {
                Some(current_reasoning_effort)
            } else {
                supported_reasoning_levels
                    .get(supported_reasoning_levels.len().saturating_sub(1) / 2)
                    .copied()
                    .or(model_info.default_reasoning_level)
            }
        } else {
            supported_reasoning_levels
                .get(supported_reasoning_levels.len().saturating_sub(1) / 2)
                .copied()
                .or(model_info.default_reasoning_level)
        };
        config.model_reasoning_effort = reasoning_effort;

        let collaboration_mode = self.collaboration_mode.with_updates(
            Some(model.clone()),
            Some(reasoning_effort),
            /*developer_instructions*/ None,
        );
        let features = self.features.clone();
        let tools_config = ToolsConfig::new(&ToolsConfigParams {
            model_info: &model_info,
            available_models: &models_manager
                .list_models_for_config(&config, RefreshStrategy::OnlineIfUncached)
                .await,
            features: &features,
            web_search_mode: self.tools_config.web_search_mode,
            session_source: self.session_source.clone(),
            sandbox_policy: self.sandbox_policy.get(),
            windows_sandbox_level: self.windows_sandbox_level,
        })
        .with_tool_wire_profile(tool_wire_profile_for_wire_api(self.provider.wire_api))
        .with_tool_capabilities(self.tools_config.tool_capabilities.clone())
        .with_unified_exec_shell_mode(self.tools_config.unified_exec_shell_mode.clone())
        .with_web_search_config(self.tools_config.web_search_config.clone())
        .with_allow_login_shell(self.tools_config.allow_login_shell)
        .with_agent_type_description(crate::agent::role::spawn_tool_spec::build(
            &config.agent_roles,
        ));

        Self {
            sub_id: self.sub_id.clone(),
            trace_id: self.trace_id.clone(),
            realtime_active: self.realtime_active,
            config: Arc::new(config),
            auth_manager: self.auth_manager.clone(),
            model_info: model_info.clone(),
            session_telemetry: self
                .session_telemetry
                .clone()
                .with_model(model.as_str(), model_info.slug.as_str()),
            provider: self.provider.clone(),
            reasoning_effort,
            reasoning_summary: self.reasoning_summary,
            session_source: self.session_source.clone(),
            environment: Arc::clone(&self.environment),
            cwd: self.cwd.clone(),
            current_date: self.current_date.clone(),
            timezone: self.timezone.clone(),
            app_gateway_client_name: self.app_gateway_client_name.clone(),
            developer_instructions: self.developer_instructions.clone(),
            compact_prompt: self.compact_prompt.clone(),
            user_instructions: self.user_instructions.clone(),
            collaboration_mode,
            personality: self.personality,
            approval_policy: self.approval_policy.clone(),
            sandbox_policy: self.sandbox_policy.clone(),
            file_system_sandbox_policy: self.file_system_sandbox_policy.clone(),
            network_sandbox_policy: self.network_sandbox_policy,
            network: self.network.clone(),
            windows_sandbox_level: self.windows_sandbox_level,
            shell_environment_policy: self.shell_environment_policy.clone(),
            tools_config,
            features,
            ghost_snapshot: self.ghost_snapshot.clone(),
            final_output_json_schema: self.final_output_json_schema.clone(),
            praxis_self_exe: self.praxis_self_exe.clone(),
            praxis_linux_sandbox_exe: self.praxis_linux_sandbox_exe.clone(),
            tool_call_gate: Arc::new(ReadinessFlag::new()),
            tool_loop_guard: Arc::clone(&self.tool_loop_guard),
            truncation_policy,
            dynamic_tools: self.dynamic_tools.clone(),
            turn_metadata_state: self.turn_metadata_state.clone(),
            turn_skills: self.turn_skills.clone(),
            turn_timing_state: Arc::clone(&self.turn_timing_state),
        }
    }

    pub(crate) fn resolve_path(&self, path: Option<String>) -> PathBuf {
        path.as_ref()
            .map(PathBuf::from)
            .map_or_else(|| self.cwd.to_path_buf(), |p| self.cwd.as_path().join(p))
    }

    pub(crate) fn compact_prompt(&self) -> &str {
        self.compact_prompt
            .as_deref()
            .unwrap_or(compact::SUMMARIZATION_PROMPT)
    }

    pub(crate) fn to_turn_context_item(&self) -> TurnContextItem {
        TurnContextItem {
            turn_id: Some(self.sub_id.clone()),
            trace_id: self.trace_id.clone(),
            cwd: self.cwd.to_path_buf(),
            current_date: self.current_date.clone(),
            timezone: self.timezone.clone(),
            approval_policy: self.approval_policy.value(),
            sandbox_policy: self.sandbox_policy.get().clone(),
            network: self.turn_context_network_item(),
            model: self.model_info.slug.clone(),
            personality: self.personality,
            collaboration_mode: Some(self.collaboration_mode.clone()),
            realtime_active: Some(self.realtime_active),
            effort: self.reasoning_effort,
            summary: self.reasoning_summary,
            user_instructions: self.user_instructions.clone(),
            developer_instructions: self.developer_instructions.clone(),
            final_output_json_schema: self.final_output_json_schema.clone(),
            truncation_policy: Some(self.truncation_policy),
        }
    }

    fn turn_context_network_item(&self) -> Option<TurnContextNetworkItem> {
        let network = self
            .config
            .config_layer_stack
            .requirements()
            .network
            .as_ref()?;
        Some(TurnContextNetworkItem {
            allowed_domains: network
                .domains
                .as_ref()
                .and_then(praxis_config::NetworkDomainPermissionsToml::allowed_domains)
                .unwrap_or_default(),
            denied_domains: network
                .domains
                .as_ref()
                .and_then(praxis_config::NetworkDomainPermissionsToml::denied_domains)
                .unwrap_or_default(),
        })
    }
}

fn local_time_context() -> (String, String) {
    match iana_time_zone::get_timezone() {
        Ok(timezone) => (Local::now().format("%Y-%m-%d").to_string(), timezone),
        Err(_) => (
            Utc::now().format("%Y-%m-%d").to_string(),
            "Etc/UTC".to_string(),
        ),
    }
}

#[derive(Clone)]
pub(crate) struct SessionConfiguration {
    /// Provider identifier ("openai", "openrouter", ...).
    provider: ModelProviderInfo,

    collaboration_mode: CollaborationMode,
    model_reasoning_summary: Option<ReasoningSummaryConfig>,
    service_tier: Option<ServiceTier>,

    /// Developer instructions that supplement the base instructions.
    developer_instructions: Option<String>,

    /// Model instructions that are appended to the base instructions.
    user_instructions: Option<String>,

    /// Personality preference for the model.
    personality: Option<Personality>,

    /// Base instructions for the session.
    base_instructions: String,

    /// Compact prompt override.
    compact_prompt: Option<String>,

    /// When to escalate for approval for execution
    approval_policy: Constrained<AskForApproval>,
    approvals_reviewer: ApprovalsReviewer,
    /// How to sandbox commands executed in the system
    sandbox_policy: Constrained<SandboxPolicy>,
    file_system_sandbox_policy: FileSystemSandboxPolicy,
    network_sandbox_policy: NetworkSandboxPolicy,
    windows_sandbox_level: WindowsSandboxLevel,

    /// Absolute working directory that should be treated as the *root* of the
    /// session. All relative paths supplied by the model as well as the
    /// execution sandbox are resolved against this directory **instead** of
    /// the process-wide current working directory.
    cwd: AbsolutePathBuf,
    /// Directory containing all Praxis state for this session.
    praxis_home: PathBuf,
    /// Optional user-facing name for the thread, updated during the session.
    thread_name: Option<String>,

    // TODO(pakrym): Remove config from here
    original_config_do_not_use: Arc<Config>,
    /// Optional service name tag for session metrics.
    metrics_service_name: Option<String>,
    app_gateway_client_name: Option<String>,
    /// Source of the session (cli, vscode, exec, mcp, ...)
    session_source: SessionSource,
    dynamic_tools: Vec<DynamicToolSpec>,
    persist_extended_history: bool,
    inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    user_shell_override: Option<shell::Shell>,
}

impl SessionConfiguration {
    pub(crate) fn praxis_home(&self) -> &PathBuf {
        &self.praxis_home
    }

    fn thread_config_snapshot(&self) -> ThreadConfigSnapshot {
        ThreadConfigSnapshot {
            model: self.collaboration_mode.model().to_string(),
            model_provider_id: self.original_config_do_not_use.model_provider_id.clone(),
            service_tier: self.service_tier,
            approval_policy: self.approval_policy.value(),
            approvals_reviewer: self.approvals_reviewer,
            sandbox_policy: self.sandbox_policy.get().clone(),
            cwd: self.cwd.to_path_buf(),
            ephemeral: self.original_config_do_not_use.ephemeral,
            reasoning_effort: self.collaboration_mode.reasoning_effort(),
            personality: self.personality,
            session_source: self.session_source.clone(),
        }
    }

    pub(crate) fn apply(&self, updates: &SessionSettingsUpdate) -> ConstraintResult<Self> {
        let mut next_configuration = self.clone();
        let file_system_policy_matches_legacy = self.file_system_sandbox_policy
            == FileSystemSandboxPolicy::from_legacy_sandbox_policy(
                self.sandbox_policy.get(),
                &self.cwd,
            );
        if let Some(model_provider_id) = updates.model_provider.as_ref() {
            if model_provider_id.is_empty() {
                return Err(ConstraintError::empty_field("model_provider"));
            }

            let mut allowed_model_providers = next_configuration
                .original_config_do_not_use
                .model_providers
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            allowed_model_providers.sort();

            let provider = next_configuration
                .original_config_do_not_use
                .model_providers
                .get(model_provider_id)
                .cloned()
                .ok_or_else(|| ConstraintError::InvalidValue {
                    field_name: "model_provider",
                    candidate: model_provider_id.clone(),
                    allowed: format!("{allowed_model_providers:?}"),
                    requirement_source: RequirementSource::Unknown,
                })?;

            next_configuration.provider = provider.clone();

            let mut config = (*next_configuration.original_config_do_not_use).clone();
            config.model_provider_id = model_provider_id.clone();
            config.model_provider = provider;
            next_configuration.original_config_do_not_use = Arc::new(config);
        }
        if let Some(collaboration_mode) = updates.collaboration_mode.clone() {
            next_configuration.collaboration_mode = collaboration_mode;
        }
        if let Some(summary) = updates.reasoning_summary {
            next_configuration.model_reasoning_summary = Some(summary);
        }
        if let Some(service_tier) = updates.service_tier {
            next_configuration.service_tier = service_tier;
        }
        if let Some(personality) = updates.personality {
            next_configuration.personality = Some(personality);
        }
        if let Some(approval_policy) = updates.approval_policy {
            next_configuration.approval_policy.set(approval_policy)?;
        }
        if let Some(approvals_reviewer) = updates.approvals_reviewer {
            next_configuration.approvals_reviewer = approvals_reviewer;
        }
        let mut sandbox_policy_changed = false;
        if let Some(sandbox_policy) = updates.sandbox_policy.clone() {
            next_configuration.sandbox_policy.set(sandbox_policy)?;
            next_configuration.network_sandbox_policy =
                NetworkSandboxPolicy::from(next_configuration.sandbox_policy.get());
            sandbox_policy_changed = true;
        }
        if let Some(windows_sandbox_level) = updates.windows_sandbox_level {
            next_configuration.windows_sandbox_level = windows_sandbox_level;
        }

        let absolute_cwd = updates
            .cwd
            .as_ref()
            .map(|cwd| {
                AbsolutePathBuf::relative_to_current_dir(normalize_for_native_workdir(
                    cwd.as_path(),
                ))
                .unwrap_or_else(|e| {
                    warn!("failed to normalize update cwd: {cwd:?}: {e}");
                    self.cwd.clone()
                })
            })
            .unwrap_or_else(|| self.cwd.clone());

        let cwd_changed = absolute_cwd.as_path() != self.cwd.as_path();
        next_configuration.cwd = absolute_cwd;
        if sandbox_policy_changed || (cwd_changed && file_system_policy_matches_legacy) {
            // Preserve richer split policies across cwd-only updates; only
            // rederive when the session is already using the legacy bridge.
            next_configuration.file_system_sandbox_policy =
                FileSystemSandboxPolicy::from_legacy_sandbox_policy(
                    next_configuration.sandbox_policy.get(),
                    &next_configuration.cwd,
                );
        }
        if let Some(app_gateway_client_name) = updates.app_gateway_client_name.clone() {
            next_configuration.app_gateway_client_name = Some(app_gateway_client_name);
        }
        Ok(next_configuration)
    }
}

#[derive(Default, Clone)]
pub(crate) struct SessionSettingsUpdate {
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) approval_policy: Option<AskForApproval>,
    pub(crate) approvals_reviewer: Option<ApprovalsReviewer>,
    pub(crate) sandbox_policy: Option<SandboxPolicy>,
    pub(crate) windows_sandbox_level: Option<WindowsSandboxLevel>,
    pub(crate) model_provider: Option<String>,
    pub(crate) collaboration_mode: Option<CollaborationMode>,
    pub(crate) reasoning_summary: Option<ReasoningSummaryConfig>,
    pub(crate) service_tier: Option<Option<ServiceTier>>,
    pub(crate) final_output_json_schema: Option<Option<Value>>,
    pub(crate) personality: Option<Personality>,
    pub(crate) app_gateway_client_name: Option<String>,
}

mod agent_turn_loop;
mod event_delivery;
/// Operation handlers
mod handlers;
mod history_context;
mod main_agent_loop;
mod mcp_runtime;
mod review;
mod session_core;
mod session_startup;
mod thread_lifecycle;
mod turn_context;

pub(crate) use agent_turn_loop::agent_turn_loop;
use agent_turn_loop::effective_auto_compact_token_limit;
pub(crate) use agent_turn_loop::record_empty_model_recovery;
use main_agent_loop::main_agent_loop;
use review::errors_to_info;
use review::skills_to_info;
use review::spawn_review_thread;

use crate::memories::prompts::build_memory_tool_developer_instructions;
use sampling::SamplingRequestResult;
pub(crate) use sampling::build_prompt;
pub(crate) use sampling::built_tools;
use sampling::collect_explicit_app_ids_from_skill_items;
#[cfg(test)]
use sampling::filter_connectors_for_input;
#[cfg(test)]
use sampling::filter_praxis_apps_mcp_tools;
pub(crate) use sampling::get_last_assistant_message_from_turn;
use sampling::realtime_text_for_event;
use sampling::run_sampling_request;
#[cfg(test)]
pub(crate) use tests::make_session_and_context;
#[cfg(test)]
pub(crate) use tests::make_session_and_context_with_dynamic_tools_and_rx;
#[cfg(test)]
pub(crate) use tests::make_session_and_context_with_rx;
#[cfg(test)]
pub(crate) use tests::make_session_configuration_for_tests;

#[cfg(test)]
#[path = "praxis_tests.rs"]
mod tests;
