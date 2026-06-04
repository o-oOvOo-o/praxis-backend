use crate::bespoke_event_handling::apply_bespoke_event_handling;
use crate::command_exec::CommandExecManager;
use crate::config_api::apply_runtime_feature_enablement;
use crate::error_code::INPUT_TOO_LARGE_ERROR_CODE;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_PARAMS_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::fuzzy_file_search::FuzzyFileSearchSession;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::ConnectionRequestId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::RequestContext;
use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use crate::thread_status::ThreadWatchManager;
use crate::thread_status::resolve_thread_status;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use praxis_analytics::AnalyticsEventsClient;
use praxis_analytics::ThreadInitializationMode;
use praxis_analytics::ThreadInitializedFact;
use praxis_app_gateway_protocol::AskForApproval;
use praxis_app_gateway_protocol::DynamicToolSpec as ApiDynamicToolSpec;
use praxis_app_gateway_protocol::GitInfo as ApiGitInfo;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::SandboxMode;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequestResolvedNotification;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadArchiveParams;
use praxis_app_gateway_protocol::ThreadArchiveResponse;
use praxis_app_gateway_protocol::ThreadArchivedNotification;
use praxis_app_gateway_protocol::ThreadBackgroundTerminalsCleanParams;
use praxis_app_gateway_protocol::ThreadBackgroundTerminalsCleanResponse;
use praxis_app_gateway_protocol::ThreadClosedNotification;
use praxis_app_gateway_protocol::ThreadCompactStartParams;
use praxis_app_gateway_protocol::ThreadCompactStartResponse;
use praxis_app_gateway_protocol::ThreadControlAcquireParams;
use praxis_app_gateway_protocol::ThreadControlAcquireResponse;
use praxis_app_gateway_protocol::ThreadControlReleaseParams;
use praxis_app_gateway_protocol::ThreadControlReleaseResponse;
use praxis_app_gateway_protocol::ThreadController;
use praxis_app_gateway_protocol::ThreadControllerKind;
use praxis_app_gateway_protocol::ThreadDecrementElicitationParams;
use praxis_app_gateway_protocol::ThreadDecrementElicitationResponse;
use praxis_app_gateway_protocol::ThreadDeleteParams;
use praxis_app_gateway_protocol::ThreadDeleteResponse;
use praxis_app_gateway_protocol::ThreadForkParams;
use praxis_app_gateway_protocol::ThreadForkResponse;
use praxis_app_gateway_protocol::ThreadIncrementElicitationParams;
use praxis_app_gateway_protocol::ThreadIncrementElicitationResponse;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadLoadedListParams;
use praxis_app_gateway_protocol::ThreadLoadedListResponse;
use praxis_app_gateway_protocol::ThreadMetadataGitInfoUpdateParams;
use praxis_app_gateway_protocol::ThreadMetadataUpdateParams;
use praxis_app_gateway_protocol::ThreadMetadataUpdateResponse;
use praxis_app_gateway_protocol::ThreadNameUpdatedNotification;
use praxis_app_gateway_protocol::ThreadReadParams;
use praxis_app_gateway_protocol::ThreadReadResponse;
use praxis_app_gateway_protocol::ThreadRegenerateNameParams;
use praxis_app_gateway_protocol::ThreadRegenerateNameResponse;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadResumeResponse;
use praxis_app_gateway_protocol::ThreadRollbackParams;
use praxis_app_gateway_protocol::ThreadSetNameParams;
use praxis_app_gateway_protocol::ThreadSetNameResponse;
use praxis_app_gateway_protocol::ThreadShellCommandParams;
use praxis_app_gateway_protocol::ThreadShellCommandResponse;
use praxis_app_gateway_protocol::ThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind as ApiThreadSourceKind;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ThreadStartedNotification;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_app_gateway_protocol::ThreadUnarchiveParams;
use praxis_app_gateway_protocol::ThreadUnarchiveResponse;
use praxis_app_gateway_protocol::ThreadUnarchivedNotification;
use praxis_app_gateway_protocol::ThreadUnsubscribeParams;
use praxis_app_gateway_protocol::ThreadUnsubscribeResponse;
use praxis_app_gateway_protocol::ThreadUnsubscribeStatus;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::UserInput as ApiUserInput;
use praxis_app_gateway_protocol::build_turns_from_rollout_items;
use praxis_arg0::Arg0DispatchPaths;
use praxis_core::Cursor as RolloutCursor;
use praxis_core::ForkSnapshot;
use praxis_core::NewThread;
use praxis_core::PraxisThread;
use praxis_core::RolloutRecorder;
use praxis_core::SessionMeta;
use praxis_core::ThreadConfigSnapshot;
use praxis_core::ThreadManager;
use praxis_core::ThreadSortKey as CoreThreadSortKey;
use praxis_core::config::Config;
use praxis_core::config::ConfigOverrides;
use praxis_core::config_loader::CloudRequirementsLoadError;
use praxis_core::config_loader::CloudRequirementsLoadErrorCode;
use praxis_core::config_loader::CloudRequirementsLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::error::PraxisErr;
use praxis_core::error::Result as PraxisResult;
use praxis_core::parse_cursor;
use praxis_core::plugins::MarketplaceError;
use praxis_core::read_head_for_summary;
use praxis_core::rollout_date_parts;
use praxis_features::Feature;
use praxis_feedback::CodexFeedback;
use praxis_login::AuthManager;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::dynamic_tools::DynamicToolSpec as CoreDynamicToolSpec;
use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::AgentStatus;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::GitInfo as CoreGitInfo;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::USER_MESSAGE_BEGIN;
use praxis_protocol::protocol::W3cTraceContext;
use praxis_rollout::state_db::StateDbHandle;
use praxis_rollout::state_db::get_state_db;
use praxis_rollout::state_db::reconcile_rollout;
use praxis_state::StateRuntime;
use praxis_state::ThreadMetadata;
use praxis_state::ThreadMetadataBuilder;
use praxis_state::log_db::LogDbLayer;
use praxis_utils_json_to_toml::json_to_toml;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::FileTimes;
use std::fs::OpenOptions;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use std::time::SystemTime;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio_util::task::TaskTracker;
use toml::Value as TomlValue;
use tracing::Instrument;
use tracing::error;
use tracing::info;
use tracing::warn;

#[cfg(test)]
use praxis_app_gateway_protocol::ServerRequest;

mod account_api;
mod apps_api;
mod apps_list_helpers;
mod command_exec_api;
mod config_derivation_api;
mod feedback_api;
mod fuzzy_search_api;
mod mcp_server_api;
mod model_feature_api;
mod plugin_api;
mod plugin_app_helpers;
mod plugin_mcp_oauth;
mod processor_runtime_api;
mod request_dispatch;
mod skills_api;
mod thread_archive_api;
mod thread_control_api;
mod thread_lifecycle_api;
mod thread_listener_api;
mod thread_metadata_api;
mod thread_projection_api;
mod turn_api;
mod windows_sandbox_api;

use account_api::ActiveLogin;
use config_derivation_api::{
    collect_resume_override_mismatches, config_load_error, derive_config_for_cwd,
    derive_config_from_params, merge_persisted_resume_metadata, thread_initialized_fact,
    validate_dynamic_tools,
};
use thread_listener_api::{EnsureConversationListenerResult, ListenerTaskContext};
#[cfg(test)]
use thread_projection_api::{
    RolloutSummary, extract_rollout_summary, summary_from_state_db_metadata,
};
use thread_projection_api::{
    build_thread_from_snapshot, hydrate_rollout_summary_with_state_db,
    load_thread_summary_for_rollout, preview_from_rollout_items,
    read_summary_from_state_db_context_by_thread_id, thread_summary_to_rollout_summary,
};
pub(crate) use thread_projection_api::{
    read_rollout_items_from_rollout, read_summary_from_rollout, summary_to_thread,
};

use crate::thread_state::ThreadListenerCommand;
use crate::thread_state::ThreadState;
use crate::thread_state::ThreadStateManager;

/// Handles JSON-RPC messages for Praxis threads.
pub(crate) struct PraxisMessageProcessor {
    auth_manager: Arc<AuthManager>,
    thread_manager: Arc<ThreadManager>,
    outgoing: Arc<OutgoingMessageSender>,
    analytics_events_client: AnalyticsEventsClient,
    arg0_paths: Arg0DispatchPaths,
    config: Arc<Config>,
    cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
    runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
    cloud_requirements: Arc<RwLock<CloudRequirementsLoader>>,
    active_login: Arc<Mutex<Option<ActiveLogin>>>,
    pending_thread_unloads: Arc<Mutex<HashSet<ThreadId>>>,
    thread_state_manager: ThreadStateManager,
    thread_watch_manager: ThreadWatchManager,
    command_exec_manager: CommandExecManager,
    pending_fuzzy_searches: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    fuzzy_search_sessions: Arc<Mutex<HashMap<String, FuzzyFileSearchSession>>>,
    background_tasks: TaskTracker,
    feedback: CodexFeedback,
    log_db: Option<LogDbLayer>,
}

pub(crate) struct PraxisMessageProcessorArgs {
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) thread_manager: Arc<ThreadManager>,
    pub(crate) outgoing: Arc<OutgoingMessageSender>,
    pub(crate) analytics_events_client: AnalyticsEventsClient,
    pub(crate) arg0_paths: Arg0DispatchPaths,
    pub(crate) config: Arc<Config>,
    pub(crate) cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
    pub(crate) runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
    pub(crate) cloud_requirements: Arc<RwLock<CloudRequirementsLoader>>,
    pub(crate) feedback: CodexFeedback,
    pub(crate) log_db: Option<LogDbLayer>,
}

impl PraxisMessageProcessor {
    pub fn new(args: PraxisMessageProcessorArgs) -> Self {
        let PraxisMessageProcessorArgs {
            auth_manager,
            thread_manager,
            outgoing,
            analytics_events_client,
            arg0_paths,
            config,
            cli_overrides,
            runtime_feature_enablement,
            cloud_requirements,
            feedback,
            log_db,
        } = args;
        Self {
            auth_manager,
            thread_manager,
            outgoing: outgoing.clone(),
            analytics_events_client,
            arg0_paths,
            config,
            cli_overrides,
            runtime_feature_enablement,
            cloud_requirements,
            active_login: Arc::new(Mutex::new(None)),
            pending_thread_unloads: Arc::new(Mutex::new(HashSet::new())),
            thread_state_manager: ThreadStateManager::new(),
            thread_watch_manager: ThreadWatchManager::new_with_outgoing(outgoing),
            command_exec_manager: CommandExecManager::default(),
            pending_fuzzy_searches: Arc::new(Mutex::new(HashMap::new())),
            fuzzy_search_sessions: Arc::new(Mutex::new(HashMap::new())),
            background_tasks: TaskTracker::new(),
            feedback,
            log_db,
        }
    }
}

#[cfg(test)]
mod tests;
