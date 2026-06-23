use crate::app_backtrack::BacktrackState;
use crate::app_command::AppCommand;
use crate::app_event::AppEvent;
use crate::app_event::ExitMode;
use crate::app_event::FeedbackCategory;
use crate::app_event::RealtimeAudioDeviceKind;
#[cfg(target_os = "windows")]
use crate::app_event::WindowsSandboxEnableMode;
use crate::app_event_sender::AppEventSender;
use crate::app_gateway_session::AppGatewaySession;
use crate::app_gateway_session::AppGatewayStartedThread;
use crate::app_gateway_session::ThreadSessionState;
use crate::app_gateway_session::app_gateway_rate_limit_snapshots_to_core;
use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::bottom_pane::ApprovalRequest;
use crate::bottom_pane::FeedbackAudience;
use crate::bottom_pane::McpServerElicitationFormRequest;
use crate::chatwidget::ChatWidget;
use crate::chatwidget::ExternalEditorState;
use crate::chatwidget::ReplayKind;
use crate::cwd_prompt::CwdPromptAction;
use crate::diff_render::DiffSummary;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::external_editor;
use crate::file_search::FileSearchManager;
use crate::history_cell;
use crate::history_cell::HistoryCell;
#[cfg(not(debug_assertions))]
use crate::history_cell::UpdateAvailableHistoryCell;
use crate::model_catalog::ModelCatalog;
use crate::model_discovery::build_model_catalog;
use crate::multi_agents::format_agent_picker_item_name_for_thread;
use crate::multi_agents::next_agent_shortcut_matches;
use crate::multi_agents::previous_agent_shortcut_matches;
use crate::pager_overlay::Overlay;
use crate::render::highlight::highlight_bash_to_lines;
use crate::resume_picker::SessionPickerAction;
use crate::resume_picker::SessionSelection;
use crate::resume_picker::SessionTarget;
#[cfg(test)]
use crate::test_support::PathBufExt;
use crate::thread_pagination::loaded_thread_list_params;
use crate::thread_replay_policy::compact_visible_replay_turns;
use crate::tui;
use crate::tui::TuiEvent;
use crate::tui_config;
use crate::tui_config::TuiRuntimeConfig;
use crate::update_action::UpdateAction;
use crate::version::PRAXIS_CLI_VERSION;
use crate::workspace::WorkspaceChromeAction;
use crate::workspace::WorkspaceChromeMenu;
use crate::workspace::WorkspaceChromeMenuState;
use crate::workspace::WorkspaceGatewayEffect;
use crate::workspace::WorkspaceMainPaneEffect;
use crate::workspace::WorkspaceMenuAction;
use crate::workspace::WorkspaceOverlay;
use crate::workspace::WorkspaceState;
use crate::workspace::parse_workspace_thread_id;
use crate::workspace::workspace_chrome_menu_actions;
use crate::workspace::workspace_menu_actions;
use crate::workspace::workspace_single_line;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use praxis_ansi_escape::ansi_escape_line;
use praxis_app_gateway_client::AppGatewayRequestHandle;
use praxis_app_gateway_client::TypedRequestError;
use praxis_app_gateway_protocol::PluginReadParams;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::SkillsListResponse;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadModelChangedNotification;
use praxis_app_gateway_protocol::ThreadRollbackResponse;
use praxis_app_gateway_protocol::Turn;
use praxis_core::ModelProviderInfo;
use praxis_core::config::Config;
use praxis_core::config::ConfigBuilder;
use praxis_core::config::ConfigOverrides;
use praxis_core::config::edit::ConfigEdit;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
#[cfg(target_os = "windows")]
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_protocol::approvals::ExecApprovalRequestEvent;
use praxis_protocol::config_types::Personality;
#[cfg(target_os = "windows")]
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::FinalOutput;
use praxis_protocol::protocol::GetHistoryEntryResponseEvent;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::TokenUsage;
use praxis_terminal_detection::user_agent;
use praxis_utils_absolute_path::AbsolutePathBuf;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::unbounded_channel;
use tokio::task::JoinHandle;
use toml::Value as TomlValue;
mod active_thread_lifecycle;
mod agent_navigation;
mod app_gateway_adapter;
mod app_gateway_background;
mod app_gateway_commands;
mod app_gateway_connection;
#[path = "app_gateway_fetch.rs"]
mod app_gateway_fetch;
mod app_gateway_requests;
mod chat_widget_factory;
mod config_refresh;
mod event_dispatch;
mod external_editor_actions;
mod feature_flags;
mod key_event_handlers;
mod loaded_threads;
mod mouse_interaction;
mod pending_interactive_replay;
mod provider_policy_events;
mod runtime_settings;
mod session_navigation;
mod startup_effects;
mod status_maintenance;
mod status_theme_events;
mod terminal_reset;
mod thread_event_apply;
mod thread_event_channels;
mod thread_event_ingress;
mod thread_event_store;
mod thread_interactive_requests;
mod workspace_actions;
mod workspace_agent_picker;
mod workspace_render;
mod workspace_session_picker;
mod workspace_threads;
mod workspace_view_helpers;

use self::agent_navigation::AgentNavigationDirection;
use self::agent_navigation::AgentNavigationState;
use self::app_gateway_requests::PendingAppGatewayRequests;
use self::loaded_threads::find_loaded_subagent_threads_for_primary;
use self::mouse_interaction::MouseInteractionState;
#[cfg(test)]
use self::startup_effects::MODEL_AVAILABILITY_NUX_MAX_SHOW_COUNT;
#[cfg(test)]
use self::startup_effects::StartupTooltipOverride;
use self::startup_effects::emit_project_config_warnings;
use self::startup_effects::emit_skill_load_warnings;
use self::startup_effects::emit_system_bwrap_warning;
use self::startup_effects::errors_for_cwd;
use self::startup_effects::handle_model_migration_prompt_if_needed;
use self::startup_effects::list_skills_response_to_core;
use self::startup_effects::prepare_startup_tooltip_override;
#[cfg(test)]
use self::startup_effects::select_model_availability_nux;
#[cfg(test)]
use self::startup_effects::should_show_model_migration_prompt;
#[cfg(test)]
use self::startup_effects::target_preset_for_upgrade;
use self::thread_event_store::FeedbackThreadEvent;
use self::thread_event_store::ThreadBufferedEvent;
use self::thread_event_store::ThreadEventChannel;
use self::thread_event_store::ThreadEventSnapshot;
use self::thread_event_store::ThreadEventStore;
use self::workspace_view_helpers::next_char_boundary;
use self::workspace_view_helpers::previous_char_boundary;

const EXTERNAL_EDITOR_HINT: &str = "Save and close external editor to continue.";
const APP_GATEWAY_EVENT_DRAIN_BUDGET: usize = 512;

enum ThreadInteractiveRequest {
    Approval(ApprovalRequest),
    McpServerElicitation(McpServerElicitationFormRequest),
}

/// Extracts `receiver_thread_ids` from collab agent tool-call notifications.
///
/// Only `ItemStarted` and `ItemCompleted` notifications with a `CollabAgentToolCall` item carry
/// receiver thread ids. All other notification variants return `None`.
fn collab_receiver_thread_ids(notification: &ServerNotification) -> Option<&[String]> {
    match notification {
        ServerNotification::ItemStarted(notification) => match &notification.item {
            ThreadItem::CollabAgentToolCall {
                receiver_thread_ids,
                ..
            } => Some(receiver_thread_ids),
            _ => None,
        },
        ServerNotification::ItemCompleted(notification) => match &notification.item {
            ThreadItem::CollabAgentToolCall {
                receiver_thread_ids,
                ..
            } => Some(receiver_thread_ids),
            _ => None,
        },
        _ => None,
    }
}

fn default_exec_approval_decisions(
    network_approval_context: Option<&praxis_protocol::protocol::NetworkApprovalContext>,
    proposed_execpolicy_amendment: Option<&praxis_protocol::approvals::ExecPolicyAmendment>,
    proposed_network_policy_amendments: Option<
        &[praxis_protocol::approvals::NetworkPolicyAmendment],
    >,
    additional_permissions: Option<&praxis_protocol::models::PermissionProfile>,
) -> Vec<praxis_protocol::protocol::ReviewDecision> {
    ExecApprovalRequestEvent::default_available_decisions(
        network_approval_context,
        proposed_execpolicy_amendment,
        proposed_network_policy_amendments,
        additional_permissions,
    )
}

/// Baseline cadence for periodic stream commit animation ticks.
///
/// Smooth-mode streaming drains one line per tick, so this interval controls
/// perceived typing speed for non-backlogged output.
const COMMIT_ANIMATION_TICK: Duration = tui::TARGET_FRAME_INTERVAL;

#[derive(Debug, Clone)]
pub struct AppExitInfo {
    pub token_usage: TokenUsage,
    pub thread_id: Option<ThreadId>,
    pub thread_name: Option<String>,
    pub update_action: Option<UpdateAction>,
    pub exit_reason: ExitReason,
}

impl AppExitInfo {
    pub fn fatal(message: impl Into<String>) -> Self {
        Self {
            token_usage: TokenUsage::default(),
            thread_id: None,
            thread_name: None,
            update_action: None,
            exit_reason: ExitReason::Fatal(message.into()),
        }
    }
}

#[derive(Debug)]
pub(crate) enum AppRunControl {
    Continue,
    Exit(ExitReason),
}

#[derive(Debug, Clone)]
pub enum ExitReason {
    UserRequested,
    Fatal(String),
}

fn session_summary(
    token_usage: TokenUsage,
    thread_id: Option<ThreadId>,
    thread_name: Option<String>,
) -> Option<SessionSummary> {
    if token_usage.is_zero() {
        return None;
    }

    let usage_line = FinalOutput::from(token_usage).to_string();
    let resume_command = praxis_core::util::resume_command(thread_name.as_deref(), thread_id);
    Some(SessionSummary {
        usage_line,
        resume_command,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSummary {
    usage_line: String,
    resume_command: Option<String>,
}

pub(crate) struct App {
    model_catalog: Arc<ModelCatalog>,
    pub(crate) session_telemetry: SessionTelemetry,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) chat_widget: ChatWidget,
    /// Config is stored here so we can recreate ChatWidgets as needed.
    pub(crate) config: Config,
    pub(crate) tui_config: TuiRuntimeConfig,
    pub(crate) active_profile: Option<String>,
    cli_kv_overrides: Vec<(String, TomlValue)>,
    harness_overrides: ConfigOverrides,
    runtime_approval_policy_override: Option<AskForApproval>,
    runtime_sandbox_policy_override: Option<SandboxPolicy>,

    pub(crate) file_search: FileSearchManager,

    pub(crate) transcript_cells: Vec<Arc<dyn HistoryCell>>,

    // Pager overlay state (Transcript or Static like Diff)
    pub(crate) overlay: Option<Overlay>,
    pub(crate) deferred_history_lines: Vec<Line<'static>>,
    has_emitted_history_lines: bool,

    pub(crate) enhanced_keys_supported: bool,

    /// Controls the animation thread that sends CommitTick events.
    pub(crate) commit_anim_running: Arc<AtomicBool>,
    // Shared across ChatWidget instances so invalid status-line config warnings only emit once.
    status_line_invalid_items_warned: Arc<AtomicBool>,
    // Shared across ChatWidget instances so invalid terminal-title config warnings only emit once.
    terminal_title_invalid_items_warned: Arc<AtomicBool>,

    // Esc-backtracking state grouped
    pub(crate) backtrack: crate::app_backtrack::BacktrackState,
    /// When set, the next draw re-renders the transcript into terminal scrollback once.
    ///
    /// This is used after a confirmed thread rollback to ensure scrollback reflects the trimmed
    /// transcript cells.
    pub(crate) backtrack_render_pending: bool,
    pub(crate) transcript_scrollback_backfill: Option<TranscriptScrollbackBackfill>,
    pub(crate) feedback: praxis_feedback::PraxisFeedback,
    feedback_audience: FeedbackAudience,
    remote_app_gateway_url: Option<String>,
    remote_app_gateway_auth_token: Option<String>,
    app_gateway_reconnect_pending: bool,
    last_app_gateway_reconnect_attempt: Option<Instant>,
    /// Set when the user confirms an update; propagated on exit.
    pub(crate) pending_update_action: Option<UpdateAction>,

    /// Tracks the thread we intentionally shut down while exiting the app.
    ///
    /// When this matches the active thread, its `ShutdownComplete` should lead to
    /// process exit instead of being treated as an unexpected sub-agent death that
    /// triggers failover to the primary thread.
    ///
    /// This is thread-scoped state (`Option<ThreadId>`) instead of a global bool
    /// so shutdown events from other threads still take the normal failover path.
    pending_shutdown_exit_thread_id: Option<ThreadId>,

    windows_sandbox: WindowsSandboxState,

    thread_event_channels: HashMap<ThreadId, ThreadEventChannel>,
    thread_event_listener_tasks: HashMap<ThreadId, JoinHandle<()>>,
    agent_navigation: AgentNavigationState,
    active_thread_id: Option<ThreadId>,
    active_thread_rx: Option<mpsc::Receiver<ThreadBufferedEvent>>,
    primary_thread_id: Option<ThreadId>,
    last_subagent_backfill_attempt: Option<ThreadId>,
    primary_session_configured: Option<ThreadSessionState>,
    pending_primary_events: VecDeque<ThreadBufferedEvent>,
    pending_app_gateway_requests: PendingAppGatewayRequests,
    workspace: WorkspaceState,
    workspace_observed_thread_ids: HashSet<ThreadId>,
    mouse: MouseInteractionState,
    mouse_capture_resume_at: Option<Instant>,
}

pub(crate) const TRANSCRIPT_SCROLLBACK_BACKFILL_CELL_BUDGET: usize = 64;
pub(crate) const TRANSCRIPT_SCROLLBACK_BACKFILL_LINE_BUDGET: usize = 512;
const APP_GATEWAY_RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const TERMINAL_ZOOM_MOUSE_RELEASE: Duration = Duration::from_millis(900);

#[derive(Debug, Clone)]
pub(crate) struct TranscriptScrollbackBackfill {
    pub(crate) next_cell: usize,
    pub(crate) width: u16,
    pub(crate) pending_lines: VecDeque<Line<'static>>,
}

#[derive(Default)]
struct WindowsSandboxState {
    setup_started_at: Option<Instant>,
    // One-shot suppression of the next world-writable scan after user confirmation.
    skip_world_writable_scan_once: bool,
}

fn normalize_harness_overrides_for_cwd(
    mut overrides: ConfigOverrides,
    base_cwd: &Path,
) -> Result<ConfigOverrides> {
    if overrides.additional_writable_roots.is_empty() {
        return Ok(overrides);
    }

    let mut normalized = Vec::with_capacity(overrides.additional_writable_roots.len());
    for root in overrides.additional_writable_roots.drain(..) {
        let absolute = AbsolutePathBuf::resolve_path_against_base(root, base_cwd)?;
        normalized.push(absolute.into_path_buf());
    }
    overrides.additional_writable_roots = normalized;
    Ok(overrides)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderConfigWriteMode {
    UpsertIfMissing,
    ForceUpsert,
}

impl Drop for App {
    fn drop(&mut self) {
        if let Err(err) = self.chat_widget.clear_managed_terminal_title() {
            tracing::debug!(error = %err, "failed to clear terminal title on app drop");
        }
    }
}

#[cfg(test)]
mod app_tests;
