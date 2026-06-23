//! The main Praxis TUI chat surface.
//!
//! `ChatWidget` consumes protocol events, builds and updates history cells, and drives rendering
//! for both the main viewport and overlay UIs.
//!
//! The UI has both committed transcript cells (finalized `HistoryCell`s) and an in-flight active
//! cell (`ChatWidget.active_cell`) that can mutate in place while streaming (often representing a
//! coalesced exec/tool group). The transcript overlay (`Ctrl+T`) renders committed cells plus a
//! cached, render-only live tail derived from the current active cell so in-flight tool calls are
//! visible immediately.
//!
//! The transcript overlay is kept in sync by `App::overlay_forward_event`, which syncs a live tail
//! during draws using `active_cell_transcript_key()` and `active_cell_transcript_lines()`. The
//! cache key is designed to change when the active cell mutates in place or when its transcript
//! output is time-dependent so the overlay can refresh its cached tail without rebuilding it on
//! every draw.
//!
//! The bottom pane exposes a single "task running" indicator that drives the spinner and interrupt
//! hints. This module treats that indicator as derived UI-busy state: it is set while an agent turn
//! is in progress and while MCP server startup is in progress. Those lifecycles are tracked
//! independently (`agent_turn_running` and `mcp_startup_status`) and synchronized via
//! `update_task_running_state`.
//!
//! For preamble-capable models, assistant output may include commentary before
//! the final answer. During streaming we hide the status row to avoid duplicate
//! progress indicators; once commentary completes and stream queues drain, we
//! re-show it so users still see turn-in-progress state between output bursts.
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use url::Url;

use self::exec_state::{
    RunningCommand, UnifiedExecProcessSummary, UnifiedExecWaitState, UnifiedExecWaitStreak,
    is_standard_tool_call, is_unified_exec_source,
};
pub(crate) use self::notification_text::Notification;
pub(crate) use self::rate_limit_state::{
    NUDGE_MODEL_SLUG, RATE_LIMIT_SWITCH_PROMPT_THRESHOLD, RateLimitErrorKind,
    RateLimitSwitchPromptState, RateLimitWarningState, app_gateway_rate_limit_error_kind,
    core_rate_limit_error_kind, get_limits_duration,
};
use self::realtime::PendingSteerCompareKey;
use self::selfwork_plan::{
    SELFWORK_PICKER_VIEW_ID, SELFWORK_PLAN_SCAN_LIMIT, SELFWORK_STALL_LIMIT, SELFWORK_USAGE,
    discover_selfwork_plan_candidates, inspect_selfwork_plan, resolve_selfwork_plan_path,
    selfwork_prompt, selfwork_search_root,
};
use self::status_text::{
    DEFAULT_COMPOSER_PLACEHOLDER, app_gateway_goal_status_label, edited_goal_status,
    extract_first_bold, format_goal_elapsed, reasoning_status_preview,
};
use crate::SessionLookupSource;
use crate::app_command::AppCommand;
use crate::app_event::RealtimeAudioDeviceKind;
use crate::app_event::ThreadGoalSetMode;
use crate::app_gateway_core_conversions::app_gateway_collab_state_to_core;
use crate::app_gateway_core_conversions::app_gateway_collab_thread_id_to_core;
use crate::app_gateway_core_conversions::app_gateway_patch_changes_to_core;
use crate::app_gateway_core_conversions::app_gateway_request_id_to_mcp_request_id;
use crate::app_gateway_core_conversions::app_gateway_web_search_action_to_core;
use crate::app_gateway_core_conversions::exec_approval_request_from_params;
use crate::app_gateway_core_conversions::patch_approval_request_from_params;
use crate::app_gateway_core_conversions::request_permissions_from_params;
use crate::app_gateway_core_conversions::request_user_input_from_params;
use crate::app_gateway_session::ThreadSessionState;
use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::mention_codec::LinkedMention;
use crate::mention_codec::encode_history_mentions;
use crate::model_catalog::ModelCatalog;
use crate::model_discovery::ModelCatalogSelectionMetadata;
use crate::multi_agents;
use crate::resume_picker::SessionPickerAction;
use crate::status::StatusAccountDisplay;
use crate::status::StatusHistoryHandle;
use crate::status::format_directory_display;
use crate::status::format_tokens_compact;
use crate::status::rate_limit_snapshot_display_for_limit;
use crate::terminal_title::SetTerminalTitleResult;
use crate::terminal_title::clear_terminal_title;
use crate::terminal_title::set_terminal_title;
use crate::text_formatting::proper_join;
use crate::tui_config::TuiRuntimeConfig;
use crate::ui_language::UiLanguage;
use crate::version::PRAXIS_CLI_VERSION;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use praxis_app_core::thread_commands::ExternalThreadCommandAction;
use praxis_app_core::thread_commands::ExternalThreadCommandIntent;
use praxis_app_core::thread_commands::ExternalThreadCommandSource;
use praxis_app_core::thread_commands::parse_external_thread_command;
use praxis_app_gateway_protocol::AppSummary;
use praxis_app_gateway_protocol::CollabAgentState as AppGatewayCollabAgentState;
use praxis_app_gateway_protocol::CollabAgentTool;
use praxis_app_gateway_protocol::CollabAgentToolCallStatus;
use praxis_app_gateway_protocol::ErrorNotification;
use praxis_app_gateway_protocol::GuardianApprovalReviewAction;
use praxis_app_gateway_protocol::ItemCompletedNotification;
use praxis_app_gateway_protocol::ItemStartedNotification;
use praxis_app_gateway_protocol::McpServerStartupState;
use praxis_app_gateway_protocol::McpServerStatusUpdatedNotification;
use praxis_app_gateway_protocol::PraxisErrorInfo as AppGatewayPraxisErrorInfo;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadControllerKind;
use praxis_app_gateway_protocol::ThreadGoal;
use praxis_app_gateway_protocol::ThreadGoalClearedNotification;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadModelChangedNotification;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnCompletedNotification;
use praxis_app_gateway_protocol::TurnError;
use praxis_app_gateway_protocol::TurnPlanStepStatus;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_config::types::ApprovalsReviewer;
use praxis_config::types::Notifications;
use praxis_config::types::WindowsSandboxModeToml;
use praxis_core::config::Config;
use praxis_core::config::Constrained;
use praxis_core::config::ConstraintResult;
use praxis_core::config_loader::ConfigLayerStackOrdering;
use praxis_core::first_party_model_owner;
use praxis_core::project_doc::DEFAULT_PROJECT_DOC_FILENAME;
use praxis_core::skills::model::SkillMetadata;
#[cfg(target_os = "windows")]
use praxis_core::windows_sandbox::WindowsSandboxLevelExt;
use praxis_features::Feature;
use praxis_git_utils::current_branch_name;
use praxis_git_utils::get_git_repo_root;
use praxis_otel::RuntimeMetricsSummary;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use praxis_protocol::account::PlanType;
use praxis_protocol::approvals::ElicitationRequestEvent;
use praxis_protocol::config_layers::ConfigLayerSource;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::CollaborationModeMask;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::config_types::Settings;
#[cfg(target_os = "windows")]
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::items::AgentMessageContent;
use praxis_protocol::items::AgentMessageItem;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::models::local_image_label_text;
use praxis_protocol::plan_tool::PlanItemArg as UpdatePlanItemArg;
use praxis_protocol::plan_tool::StepStatus as UpdatePlanItemStatus;
#[cfg(test)]
use praxis_protocol::protocol::AgentMessageDeltaEvent;
#[cfg(test)]
use praxis_protocol::protocol::AgentMessageEvent;
#[cfg(test)]
use praxis_protocol::protocol::AgentReasoningDeltaEvent;
#[cfg(test)]
use praxis_protocol::protocol::AgentReasoningEvent;
#[cfg(test)]
use praxis_protocol::protocol::AgentReasoningRawContentDeltaEvent;
#[cfg(test)]
use praxis_protocol::protocol::AgentReasoningRawContentEvent;
use praxis_protocol::protocol::AgentStatus;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
#[cfg(test)]
use praxis_protocol::protocol::BackgroundEventEvent;
use praxis_protocol::protocol::CollabAgentRef;
#[cfg(test)]
use praxis_protocol::protocol::CollabAgentSpawnBeginEvent;
use praxis_protocol::protocol::CollabAgentStatusEntry;
use praxis_protocol::protocol::CreditsSnapshot;
#[cfg(test)]
use praxis_protocol::protocol::ErrorEvent;
#[cfg(test)]
use praxis_protocol::protocol::Event;
#[cfg(test)]
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::ExecCommandBeginEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::ExecCommandOutputDeltaEvent;
use praxis_protocol::protocol::ExecCommandSource;
#[cfg(test)]
use praxis_protocol::protocol::ExitedReviewModeEvent;
use praxis_protocol::protocol::GuardianAssessmentAction;
use praxis_protocol::protocol::GuardianAssessmentEvent;
use praxis_protocol::protocol::GuardianAssessmentStatus;
use praxis_protocol::protocol::ImageGenerationBeginEvent;
use praxis_protocol::protocol::ImageGenerationEndEvent;
use praxis_protocol::protocol::ListSkillsResponseEvent;
#[cfg(test)]
use praxis_protocol::protocol::McpListToolsResponseEvent;
#[cfg(test)]
use praxis_protocol::protocol::McpStartupCompleteEvent;
use praxis_protocol::protocol::McpStartupStatus;
#[cfg(test)]
use praxis_protocol::protocol::McpStartupUpdateEvent;
use praxis_protocol::protocol::McpToolCallBeginEvent;
use praxis_protocol::protocol::McpToolCallEndEvent;
use praxis_protocol::protocol::OPENAI_HOSTED_PRIMARY_RATE_LIMIT_ID;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::PatchApplyBeginEvent;
use praxis_protocol::protocol::PraxisErrorInfo as CorePraxisErrorInfo;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::ReviewRequest;
use praxis_protocol::protocol::ReviewTarget;
use praxis_protocol::protocol::SkillMetadata as ProtocolSkillMetadata;
#[cfg(test)]
use praxis_protocol::protocol::StreamErrorEvent;
use praxis_protocol::protocol::TerminalInteractionEvent;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;
use praxis_protocol::protocol::TurnAbortReason;
#[cfg(test)]
use praxis_protocol::protocol::TurnCompleteEvent;
#[cfg(test)]
use praxis_protocol::protocol::TurnDiffEvent;
use praxis_protocol::protocol::UserMessageEvent;
use praxis_protocol::protocol::ViewImageToolCallEvent;
#[cfg(test)]
use praxis_protocol::protocol::WarningEvent;
use praxis_protocol::protocol::WebSearchBeginEvent;
use praxis_protocol::protocol::WebSearchEndEvent;
use praxis_protocol::protocol::is_openai_hosted_primary_rate_limit;
use praxis_protocol::request_permissions::RequestPermissionsEvent;
use praxis_protocol::request_user_input::RequestUserInputEvent;
use praxis_protocol::user_input::TextElement;
use praxis_protocol::user_input::UserInput;
use praxis_terminal_detection::Multiplexer;
use praxis_terminal_detection::TerminalInfo;
use praxis_terminal_detection::TerminalName;
use praxis_terminal_detection::terminal_info;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_sleep_inhibitor::SleepInhibitor;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

const DEFAULT_MODEL_DISPLAY_NAME: &str = "loading";
const PLAN_IMPLEMENTATION_TITLE: &str = "Implement this plan?";
const PLAN_IMPLEMENTATION_YES: &str = "Yes, implement this plan";
const PLAN_IMPLEMENTATION_NO: &str = "No, stay in Plan mode";
const PLAN_IMPLEMENTATION_CODING_MESSAGE: &str = "Implement the plan.";
const PLAN_MODE_REASONING_SCOPE_TITLE: &str = "Apply reasoning change";
const PLAN_MODE_REASONING_SCOPE_PLAN_ONLY: &str = "Apply to Plan mode override";
const PLAN_MODE_REASONING_SCOPE_ALL_MODES: &str = "Apply to global default and Plan mode override";
const CONNECTORS_SELECTION_VIEW_ID: &str = "connectors-selection";
const TUI_STUB_MESSAGE: &str = "Not available in TUI yet.";
const STATUS_ACTIVITY_TEXT_MAX_GRAPHEMES: usize = 48;
const REASONING_SUMMARY_STATUS_PREVIEW_MAX_LINES: usize = 4;
const REASONING_FULL_STATUS_PREVIEW_MAX_LINES: usize = 8;
const IN_APP_TOAST_DURATION: Duration = Duration::from_secs(4);
const IN_APP_TOAST_PRIORITY_DURATION: Duration = Duration::from_secs(6);
const ACTIVE_CELL_ANIMATION_FRAME_DELAY_FOCUSED: Duration = Duration::from_millis(60);
const ACTIVE_CELL_ANIMATION_FRAME_DELAY_UNFOCUSED: Duration = Duration::from_millis(140);
const DEEPSEEK_HEADER_HEIGHT: u16 = 1;
const DEEPSEEK_FOOTER_HEIGHT: u16 = 1;
const DEEPSEEK_CHROME_MIN_HEIGHT: u16 = 6;
const WORKSPACE_ENTRY_MAX_WIDTH: u16 = 88;
const WORKSPACE_ENTRY_MIN_SIDE_PADDING: u16 = 4;
const WORKSPACE_ENTRY_INTRO_HEIGHT: u16 = 7;
const LAUNCH_STRIP_RANK_MAX: u8 = 2;
const CHAT_SURFACE_CONTENT_MAX_WIDTH: u16 = 96;
const WORKSPACE_INPUT_BORDER_ROWS: u16 = 1;
const WORKSPACE_INPUT_BORDER_COLS: u16 = 2;
const WORKSPACE_INPUT_STRIP_ROWS: u16 = 1;
const MULTI_AGENT_ENABLE_TITLE: &str = "Enable subagents?";
const MULTI_AGENT_ENABLE_YES: &str = "Enable subagents";
const MULTI_AGENT_ENABLE_NO: &str = "Keep disabled";
const MULTI_AGENT_ENABLE_NOTICE: &str =
    "Subagents are enabled. Start a new session to use them.";

/// Choose the keybinding used to edit the most-recently queued message.
///
/// Apple Terminal, Warp, and VSCode integrated terminals intercept or silently
/// swallow Alt+Up, and tmux does not reliably pass that chord through. We fall
/// back to Shift+Left for those environments while keeping the more discoverable
/// Alt+Up everywhere else.
///
/// The match is exhaustive so that adding a new `TerminalName` variant forces
/// an explicit decision about which binding that terminal should use.
fn queued_message_edit_binding_for_terminal(terminal_info: TerminalInfo) -> KeyBinding {
    if matches!(
        terminal_info.multiplexer.as_ref(),
        Some(Multiplexer::Tmux { .. })
    ) {
        return key_hint::shift(KeyCode::Left);
    }

    match terminal_info.name {
        TerminalName::AppleTerminal | TerminalName::WarpTerminal | TerminalName::VsCode => {
            key_hint::shift(KeyCode::Left)
        }
        TerminalName::Ghostty
        | TerminalName::Iterm2
        | TerminalName::WezTerm
        | TerminalName::Kitty
        | TerminalName::Alacritty
        | TerminalName::Konsole
        | TerminalName::GnomeTerminal
        | TerminalName::Vte
        | TerminalName::WindowsTerminal
        | TerminalName::Dumb
        | TerminalName::Unknown => key_hint::alt(KeyCode::Up),
    }
}

use crate::app_event::AppEvent;
use crate::app_event::ConnectorsSnapshot;
use crate::app_event::ExitMode;
#[cfg(target_os = "windows")]
use crate::app_event::WindowsSandboxEnableMode;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ApprovalRequest;
use crate::bottom_pane::BottomPane;
use crate::bottom_pane::BottomPaneParams;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::CollaborationModeIndicator;
use crate::bottom_pane::InputResult;
use crate::bottom_pane::LocalImageAttachment;
use crate::bottom_pane::McpServerElicitationFormRequest;
use crate::bottom_pane::MentionBinding;
use crate::bottom_pane::QUIT_SHORTCUT_TIMEOUT;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::custom_prompt_view::CustomPromptView;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::bottom_pane::StatusLineItem;
use crate::bottom_pane::TerminalTitleItem;
use crate::clipboard_paste::paste_image_to_temp_png;
use crate::clipboard_text;
use crate::collaboration_modes;
use crate::diff_render::display_path_for;
use crate::exec_cell::CommandOutput;
use crate::exec_cell::ExecCell;
use crate::exec_cell::new_active_exec_command;
use crate::exec_command::split_command_string;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::get_git_diff::get_git_diff;
use crate::history_cell;
#[cfg(test)]
use crate::history_cell::AgentMessageCell;
use crate::history_cell::ChatLane;
use crate::history_cell::HistoryCell;
use crate::history_cell::McpToolCallCell;
use crate::history_cell::PlainHistoryCell;
use crate::history_cell::WebSearchCell;
use crate::key_hint;
use crate::key_hint::KeyBinding;
#[cfg(test)]
use crate::markdown::append_markdown;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
use crate::slash_command::SlashCommand;
use crate::status::RateLimitSnapshotDisplay;
use crate::status::RateLimitWindowDisplay;
use crate::status_indicator_widget::STATUS_DETAILS_DEFAULT_MAX_LINES;
use crate::status_indicator_widget::StatusDetailsCapitalization;
use crate::status_runtime::GENERIC_STATUS_HEADER;
use crate::text_formatting::truncate_text;
use crate::thinking_persona::ThinkingPersona;
use crate::toast_queue::ToastEntry;
use crate::toast_queue::ToastQueue;
use crate::toast_queue::ToastSeverity;
use crate::transcript_search::TranscriptSearchState;
use crate::tui::FrameRequester;
use crate::turn_runtime::TurnRuntimeState;
use crate::workspace::LaunchStripState;
use crate::workspace::WorkPanelContextState;
use crate::workspace::WorkPanelControlState;
use crate::workspace::WorkPanelGoalState;
use crate::workspace::WorkPanelGoalStatus;
use crate::workspace::WorkPanelQueueState;
use crate::workspace::WorkPanelState;
use crate::workspace::WorkspaceTranscriptCache;
use crate::workspace::theme as workspace_theme;
mod app_command_bridge;
mod assistant_stream;
mod bottom_pane_surfaces;
mod collab_events;
mod collab_metadata;
mod command_dispatch;
mod composer_access;
mod composer_ui;
mod connectors_ui;
mod construction;
mod event_effects;
mod event_mapping;
mod exec_state;
mod experimental_settings;
mod history_output;
mod input_recovery;
mod interrupts;
mod keyboard_shortcuts;
mod mcp_startup_state;
mod message_submission;
mod recording_meter;
mod thread_control_surface;
mod widget_controls;
use self::connectors_ui::ConnectorsCacheState;
pub(crate) use self::event_mapping::ReplayKind;
use self::event_mapping::ThreadItemRenderSource;
use self::event_mapping::app_gateway_collab_agent_statuses_to_core;
use self::event_mapping::app_gateway_collab_receiver_agent_refs;
use self::event_mapping::hook_completed_event_from_notification;
use self::event_mapping::hook_started_event_from_notification;
use self::event_mapping::session_state_to_configured_event;
use self::interrupts::InterruptManager;
mod model_picker;
mod notification_text;
mod permissions_picker;
mod praxis_event_replay;
mod session_header;
mod session_lifecycle;
mod session_settings;
mod state_types;
use self::session_header::SessionHeader;
mod skills;
use self::skills::collect_tool_mentions;
use self::skills::find_app_mentions;
use self::skills::find_skill_mentions_with_tool_mentions;
mod plugins;
use self::plugins::PluginsCacheState;
mod provider_login;
mod rate_limit_state;
mod realtime;
mod realtime_audio_settings;
use self::realtime::RealtimeConversationUiState;
use self::realtime::RenderedUserMessageEvent;
mod review_picker;
#[cfg(test)]
pub(crate) use self::review_picker::show_review_commit_picker_with_entries;
mod resume_replay;
mod security_prompts;
use self::resume_replay::ResumeReplayProjector;
mod selfwork_controller;
mod selfwork_plan;
mod server_events;
mod status_controller;
mod status_surfaces;
mod status_text;
mod stream_commit;
mod surface_capabilities;
mod surface_layout;
mod thread_replay;
mod token_status;
mod tool_event_effects;
mod tool_events;
mod transcript_search_support;
mod turn_completion;
mod user_message;
mod workspace_launch_strip;
mod workspace_surface_render;
mod workspace_transcript_render;
use self::state_types::ActiveCellRenderCache;
use self::state_types::ActiveCellRenderCacheKey;
pub(crate) use self::state_types::ActiveCellTranscriptKey;
use self::state_types::CollabAgentMetadata;
pub(crate) use self::state_types::ExternalEditorState;
use self::state_types::PendingGuardianReviewStatus;
use self::state_types::PluginInstallAuthFlowState;
use self::state_types::PluginListFetchState;
use self::state_types::PraxisOpTarget;
use self::state_types::ReasoningBlockKind;
use self::state_types::StatusIndicatorState;
use self::state_types::WorkspaceActiveTailCache;
use self::state_types::WorkspaceReasoningChoice;
use self::state_types::thread_control_display_label;
use self::status_surfaces::CachedProjectRootName;
use self::status_surfaces::TerminalTitleStatusKind;
use self::transcript_search_support::TranscriptSearchDocumentCache;
use self::user_message::PendingSteer;
use self::user_message::ThreadComposerState;
pub(crate) use self::user_message::ThreadInputState;
pub(crate) use self::user_message::UserMessage;
pub(crate) use self::user_message::create_initial_user_message;
use self::user_message::merge_user_messages;
use crate::streaming::chunking::AdaptiveChunkingPolicy;
use crate::streaming::commit_tick::CommitTickScope;
use crate::streaming::commit_tick::run_commit_tick;
use crate::streaming::controller::PlanStreamController;
use crate::streaming::controller::StreamController;

use chrono::Local;
use praxis_file_search::FileMatch;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::plan_tool::StepStatus;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_approval_presets::ApprovalPreset;
use praxis_utils_approval_presets::builtin_approval_presets;
use strum::IntoEnumIterator;
use unicode_segmentation::UnicodeSegmentation;

const USER_SHELL_COMMAND_HELP_TITLE: &str = "Prefix a command with ! to run it locally";
const USER_SHELL_COMMAND_HELP_HINT: &str = "Example: !ls";
const FAST_STATUS_MODEL: &str = "gpt-5.4";
const DEFAULT_STATUS_LINE_ITEMS: [&str; 0] = [];
/// Common initialization parameters shared by all `ChatWidget` constructors.
pub(crate) struct ChatWidgetInit {
    pub(crate) config: Config,
    pub(crate) tui_config: TuiRuntimeConfig,
    pub(crate) frame_requester: FrameRequester,
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) initial_user_message: Option<UserMessage>,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) has_chatgpt_account: bool,
    pub(crate) model_catalog: Arc<ModelCatalog>,
    pub(crate) feedback: praxis_feedback::PraxisFeedback,
    pub(crate) is_first_run: bool,
    pub(crate) status_account_display: Option<StatusAccountDisplay>,
    pub(crate) initial_plan_type: Option<PlanType>,
    pub(crate) model: Option<String>,
    pub(crate) startup_tooltip_override: Option<String>,
    // Shared latch so we only warn once about invalid status-line item IDs.
    pub(crate) status_line_invalid_items_warned: Arc<AtomicBool>,
    // Shared latch so we only warn once about invalid terminal-title item IDs.
    pub(crate) terminal_title_invalid_items_warned: Arc<AtomicBool>,
    pub(crate) session_telemetry: SessionTelemetry,
}

/// Maintains the per-session UI state and interaction state machines for the chat screen.
///
/// `ChatWidget` owns the state derived from the protocol event stream (history cells, streaming
/// buffers, bottom-pane overlays, and transient status text) and turns key presses into user
/// intent (`Op` submissions and `AppEvent` requests).
///
/// It is not responsible for running the agent itself; it reflects progress by updating UI state
/// and by sending requests back to praxis-core.
///
/// Quit/interrupt behavior intentionally spans layers: the bottom pane owns local input routing
/// (which view gets Ctrl+C), while `ChatWidget` owns process-level decisions such as interrupting
/// active work, arming the double-press quit shortcut, and requesting shutdown-first exit.
pub(crate) struct ChatWidget {
    app_event_tx: AppEventSender,
    praxis_op_target: PraxisOpTarget,
    bottom_pane: BottomPane,
    active_cell: Option<Box<dyn HistoryCell>>,
    /// Monotonic-ish counter used to invalidate transcript overlay caching.
    ///
    /// The transcript overlay appends a cached "live tail" for the current active cell. Most
    /// active-cell updates are mutations of the *existing* cell (not a replacement), so pointer
    /// identity alone is not a good cache key.
    ///
    /// Callers bump this whenever the active cell's transcript output could change without
    /// flushing. It is intentionally allowed to wrap, which implies a rare one-time cache collision
    /// where the overlay may briefly treat new tail content as already cached.
    active_cell_revision: u64,
    /// First-class transcript search model used by the transcript overlay.
    ///
    /// `ChatWidget` owns the active-cell tail and exposes transcript-derived helpers, while `App`
    /// owns committed transcript cells. The app layer passes committed cells into the helpers below
    /// so query state, result counters, and next/prev navigation can still live with the chat UI.
    transcript_search: TranscriptSearchState,
    transcript_search_document_cache: Option<TranscriptSearchDocumentCache>,
    config: Config,
    tui_config: TuiRuntimeConfig,
    ui_language: UiLanguage,
    /// The unmasked collaboration mode settings (always Default mode).
    ///
    /// Masks are applied on top of this base mode to derive the effective mode.
    current_collaboration_mode: CollaborationMode,
    /// The currently active collaboration mask, if any.
    active_collaboration_mask: Option<CollaborationModeMask>,
    has_chatgpt_account: bool,
    model_catalog: Arc<ModelCatalog>,
    session_telemetry: SessionTelemetry,
    session_header: SessionHeader,
    initial_user_message: Option<UserMessage>,
    status_account_display: Option<StatusAccountDisplay>,
    token_info: Option<TokenUsageInfo>,
    thread_control_state: Option<ThreadControlState>,
    rate_limit_snapshots_by_limit_id: BTreeMap<String, RateLimitSnapshotDisplay>,
    refreshing_status_outputs: Vec<(u64, StatusHistoryHandle)>,
    next_status_refresh_request_id: u64,
    plan_type: Option<PlanType>,
    rate_limit_warnings: RateLimitWarningState,
    rate_limit_switch_prompt: RateLimitSwitchPromptState,
    adaptive_chunking: AdaptiveChunkingPolicy,
    // Stream lifecycle controller
    stream_controller: Option<StreamController>,
    // Stream lifecycle controller for proposed plan output.
    plan_stream_controller: Option<PlanStreamController>,
    // Latest completed user-visible Praxis output that `/copy` should place on the clipboard.
    last_copyable_output: Option<String>,
    // Final-answer agent message observed during the active turn. App-gateway turn completion
    // notifications do not repeat this payload, so we promote it when the turn completes.
    pending_turn_copyable_output: Option<String>,
    running_commands: HashMap<String, RunningCommand>,
    collab_agent_metadata: HashMap<ThreadId, CollabAgentMetadata>,
    pending_collab_spawn_requests: HashMap<String, multi_agents::SpawnRequestSummary>,
    suppressed_exec_calls: HashSet<String>,
    skills_all: Vec<ProtocolSkillMetadata>,
    skills_initial_state: Option<HashMap<PathBuf, bool>>,
    last_unified_wait: Option<UnifiedExecWaitState>,
    unified_exec_wait_streak: Option<UnifiedExecWaitStreak>,
    turn_sleep_inhibitor: SleepInhibitor,
    task_complete_pending: bool,
    unified_exec_processes: Vec<UnifiedExecProcessSummary>,
    /// Tracks whether praxis-core currently considers an agent turn to be in progress.
    ///
    /// This is kept separate from `mcp_startup_status` so that MCP startup progress (or completion)
    /// can update the status header without accidentally clearing the spinner for an active turn.
    agent_turn_running: bool,
    /// Tracks per-server MCP startup state while startup is in progress.
    ///
    /// The map is `Some(_)` from the first `McpStartupUpdate` until `McpStartupComplete`, and the
    /// bottom pane is treated as "running" while this is populated, even if no agent turn is
    /// currently executing.
    mcp_startup_status: Option<HashMap<String, McpStartupStatus>>,
    /// Expected MCP servers for the current startup round, seeded from enabled local config.
    mcp_startup_expected_servers: Option<HashSet<String>>,
    /// After startup settles, ignore stale updates until enough notifications confirm a new round.
    mcp_startup_ignore_updates_until_next_start: bool,
    /// A lag signal for the next round means terminal-only updates are enough to settle it.
    mcp_startup_allow_terminal_only_next_round: bool,
    /// Buffers post-settle MCP startup updates until they cover a full fresh round.
    mcp_startup_pending_next_round: HashMap<String, McpStartupStatus>,
    /// Tracks whether the buffered next round has seen any `Starting` update yet.
    mcp_startup_pending_next_round_saw_starting: bool,
    connectors_cache: ConnectorsCacheState,
    connectors_partial_snapshot: Option<ConnectorsSnapshot>,
    connectors_prefetch_in_flight: bool,
    connectors_force_refetch_pending: bool,
    plugins_cache: PluginsCacheState,
    plugins_fetch_state: PluginListFetchState,
    plugin_install_apps_needing_auth: Vec<AppSummary>,
    plugin_install_auth_flow: Option<PluginInstallAuthFlowState>,
    // Queue of interruptive UI events deferred during an active write cycle
    interrupts: InterruptManager,
    // Accumulates the current reasoning block text to extract a header
    reasoning_buffer: String,
    // Accumulates full reasoning content for transcript-only recording
    full_reasoning_buffer: String,
    reasoning_block_kind: Option<ReasoningBlockKind>,
    // The currently rendered footer state. We keep the already-formatted
    // details here so transient stream interruptions can restore the footer
    // exactly as it was shown.
    current_status: StatusIndicatorState,
    /// Runtime snapshot for active task, status overrides, activity, and footer copy.
    turn_status_snapshot: TurnRuntimeState,
    // Guardian review keeps its own pending set so it can derive a single
    // footer summary from one or more in-flight review events.
    pending_guardian_review_status: PendingGuardianReviewStatus,
    // Semantic status used for terminal-title status rendering.
    terminal_title_status_kind: TerminalTitleStatusKind,
    // Previous status header to restore after a transient stream retry.
    retry_status_header: Option<String>,
    // Set when commentary output completes; once stream queues go idle we restore the status row.
    pending_status_indicator_restore: bool,
    suppress_queue_autosend: bool,
    thread_id: Option<ThreadId>,
    thread_name: Option<String>,
    forked_from: Option<ThreadId>,
    frame_requester: FrameRequester,
    // Whether to include the initial welcome banner on session configured
    show_welcome_banner: bool,
    // One-shot tooltip override for the primary startup session.
    startup_tooltip_override: Option<String>,
    // When resuming an existing session (selected via resume picker), avoid an
    // immediate redraw on SessionConfigured to prevent a gratuitous UI flicker.
    suppress_session_configured_redraw: bool,
    // During snapshot restore, defer startup prompt submission until replayed
    // history has been rendered so resumed/forked prompts keep chronological
    // order.
    suppress_initial_user_message_submit: bool,
    // User messages queued while a turn is in progress
    queued_user_messages: VecDeque<UserMessage>,
    // User messages that tried to steer a non-regular turn and must be retried first.
    rejected_steers_queue: VecDeque<UserMessage>,
    // Steers already submitted to core but not yet committed into history.
    //
    // The bottom pane shows these above queued drafts until core records the
    // corresponding user message item.
    pending_steers: VecDeque<PendingSteer>,
    // When set, the next interrupt should resubmit all pending steers as one
    // fresh user turn instead of restoring them into the composer.
    submit_pending_steers_after_interrupt: bool,
    // Pending cross-thread approvals mirrored for the TUI work dashboard.
    pending_thread_approvals_count: usize,
    /// Terminal-appropriate keybinding for popping the most-recently queued
    /// message back into the composer.  Determined once at construction time via
    /// [`queued_message_edit_binding_for_terminal`] and propagated to
    /// `BottomPane` so the hint text matches the actual shortcut.
    queued_message_edit_binding: KeyBinding,
    // Pending notification to show when unfocused on next Draw
    pending_notification: Option<Notification>,
    in_app_toasts: ToastQueue,
    /// When `Some`, the user has pressed a quit shortcut and the second press
    /// must occur before `quit_shortcut_expires_at`.
    quit_shortcut_expires_at: Option<Instant>,
    /// Tracks which quit shortcut key was pressed first.
    ///
    /// We require the second press to match this key so `Ctrl+C` followed by
    /// `Ctrl+D` (or vice versa) doesn't quit accidentally.
    quit_shortcut_key: Option<KeyBinding>,
    // Simple review mode flag; used to adjust layout and banners.
    is_review_mode: bool,
    // Snapshot of token usage to restore after review mode exits.
    pre_review_token_info: Option<Option<TokenUsageInfo>>,
    // Whether the next streamed assistant content should be preceded by a final message separator.
    //
    // This is set whenever we insert a visible history cell that conceptually belongs to a turn.
    // The separator itself is only rendered if the turn recorded "work" activity (see
    // `had_work_activity`).
    needs_final_message_separator: bool,
    // Whether the current turn performed "work" (exec commands, MCP tool calls, patch applications).
    //
    // This gates rendering of the "Worked for …" separator so purely conversational turns don't
    // show an empty divider. It is reset when the separator is emitted.
    had_work_activity: bool,
    // Whether the current turn emitted a plan update.
    saw_plan_update_this_turn: bool,
    // Whether the current turn emitted a proposed plan item that has not been superseded by a
    // later steer. This is cleared when the user submits a steer so the plan popup only appears
    // if a newer proposed plan arrives afterward.
    saw_plan_item_this_turn: bool,
    // Latest `update_plan` checklist task counts for terminal-title rendering.
    last_plan_progress: Option<(usize, usize)>,
    // TUI-only work sidebar projection; runtime ownership stays in core/protocol state.
    work_panel: WorkPanelState,
    // Incremental buffer for streamed plan content.
    plan_delta_buffer: String,
    // True while a plan item is streaming.
    plan_item_active: bool,
    // Status-indicator elapsed seconds captured at the last emitted final-message separator.
    //
    // This lets the separator show per-chunk work time (since the previous separator) rather than
    // the total task-running time reported by the status indicator.
    last_separator_elapsed_secs: Option<u64>,
    // Runtime metrics accumulated across delta snapshots for the active turn.
    turn_runtime_metrics: RuntimeMetricsSummary,
    last_rendered_width: Cell<Option<usize>>,
    last_visible_patch_cell_ids: RefCell<Vec<crate::history_presentation::PatchCellId>>,
    active_cell_render_cache: RefCell<Option<ActiveCellRenderCache>>,
    workspace_active_tail_cache: RefCell<Option<WorkspaceActiveTailCache>>,
    workspace_transcript_cache: RefCell<WorkspaceTranscriptCache>,
    // Feedback sink for /feedback
    feedback: praxis_feedback::PraxisFeedback,
    // Current session rollout path (if known)
    current_rollout_path: Option<PathBuf>,
    // Current working directory (if known)
    current_cwd: Option<PathBuf>,
    // Active markdown plan that selfwork should keep advancing when the thread is idle.
    selfwork_plan_path: Option<PathBuf>,
    // Digest of the plan file before the current selfwork turn started.
    selfwork_last_plan_digest: Option<u64>,
    // Number of consecutive selfwork turns that left the plan file unchanged.
    selfwork_stall_count: u8,
    // True while the current in-flight turn was launched by selfwork.
    selfwork_turn_in_flight: bool,
    // Runtime network proxy bind addresses from SessionConfigured.
    session_network_proxy: Option<praxis_protocol::protocol::SessionNetworkProxyRuntime>,
    // Shared latch so we only warn once about invalid status-line item IDs.
    status_line_invalid_items_warned: Arc<AtomicBool>,
    // Shared latch so we only warn once about invalid terminal-title item IDs.
    terminal_title_invalid_items_warned: Arc<AtomicBool>,
    // Last terminal title emitted, to avoid writing duplicate OSC updates.
    pub(crate) last_terminal_title: Option<String>,
    // Original terminal-title config captured when the setup UI opens.
    //
    // The outer `Option` tracks whether a setup session is active (`Some`)
    // or not (`None`). The inner `Option<Vec<String>>` mirrors the shape
    // of `tui_config.terminal_title` (which is `None` when using defaults).
    // On cancel or persist-failure the inner value is restored to config;
    // on confirm the outer is set to `None` to end the session.
    terminal_title_setup_original_items: Option<Option<Vec<String>>>,
    // Baseline instant used to animate spinner-prefixed title statuses.
    terminal_title_animation_origin: Instant,
    // Cached project-root display name keyed by cwd for status/title rendering.
    status_line_project_root_name_cache: Option<CachedProjectRootName>,
    // Cached git branch name for the status line (None if unknown).
    status_line_branch: Option<String>,
    // CWD used to resolve the cached branch; change resets branch state.
    status_line_branch_cwd: Option<PathBuf>,
    // True while an async branch lookup is in flight.
    status_line_branch_pending: bool,
    // True once we've attempted a branch lookup for the current CWD.
    status_line_branch_lookup_complete: bool,
    external_editor_state: ExternalEditorState,
    realtime_conversation: RealtimeConversationUiState,
    last_rendered_user_message_event: Option<RenderedUserMessageEvent>,
    last_non_retry_error: Option<(String, String)>,
    pending_goal_completion_elapsed: Option<String>,
}

impl Drop for ChatWidget {
    fn drop(&mut self) {
        self.reset_realtime_conversation_state();
        self.stop_rate_limit_poller();
    }
}

#[cfg(test)]
pub(crate) mod tests;
