//! The bottom pane is the interactive footer of the chat UI.
//!
//! The pane owns the [`ChatComposer`] (editable prompt input) and a stack of transient
//! [`BottomPaneView`]s (popups/modals) that temporarily replace the composer for focused
//! interactions like selection lists.
//!
//! Input routing is layered: `BottomPane` decides which local surface receives a key (view vs
//! composer), while higher-level intent such as "interrupt" or "quit" is decided by the parent
//! widget (`ChatWidget`). This split matters for Ctrl+C/Ctrl+D: the bottom pane gives the active
//! view the first chance to consume Ctrl+C (typically to dismiss itself), and `ChatWidget` may
//! treat an unhandled Ctrl+C as an interrupt or as the first press of a double-press quit
//! shortcut.
//!
//! Some UI is time-based rather than input-based, such as the transient "press again to quit"
//! hint. The pane schedules redraws so those hints can expire even when the UI is otherwise idle.
use std::path::PathBuf;

use crate::app_event::ConnectorsSnapshot;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::pending_input_preview::PendingInputPreview;
use crate::bottom_pane::pending_thread_approvals::PendingThreadApprovals;
use crate::bottom_pane::unified_exec_footer::UnifiedExecFooter;
use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::surface::SurfaceTheme;
use crate::thinking_persona::ThinkingPersona;
use crate::tui::FrameRequester;
use bottom_pane_view::BottomPaneView;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::MouseEvent;
use praxis_core::plugins::PluginCapabilitySummary;
use praxis_core::skills::model::SkillMetadata;
use praxis_features::Features;
use praxis_file_search::FileMatch;
use praxis_protocol::request_user_input::RequestUserInputEvent;
use praxis_protocol::user_input::TextElement;
use ratatui::text::Line;
use std::time::Duration;

mod app_link_view;
mod approval_overlay;
mod mcp_server_elicitation;
mod multi_select_picker;
mod request_user_input;
mod status_line_setup;
mod title_setup;
pub(crate) use app_link_view::AppLinkElicitationTarget;
pub(crate) use app_link_view::AppLinkSuggestionType;
pub(crate) use app_link_view::AppLinkView;
pub(crate) use app_link_view::AppLinkViewParams;
pub(crate) use approval_overlay::ApprovalOverlay;
pub(crate) use approval_overlay::ApprovalRequest;
pub(crate) use approval_overlay::format_requested_permissions_rule;
pub(crate) use mcp_server_elicitation::McpServerElicitationFormRequest;
pub(crate) use mcp_server_elicitation::McpServerElicitationOverlay;
pub(crate) use request_user_input::RequestUserInputOverlay;
mod bottom_pane_view;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalImageAttachment {
    pub(crate) placeholder: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MentionBinding {
    /// Mention token text without the leading `$`.
    pub(crate) mention: String,
    /// Canonical mention target (for example `app://...` or absolute SKILL.md path).
    pub(crate) path: String,
}
mod chat_composer;
mod chat_composer_history;
mod command_popup;
pub mod custom_prompt_view;
mod experimental_features_view;
mod file_search_popup;
mod footer;
mod list_selection_view;
mod prompt_args;
mod skill_popup;
mod skills_toggle_view;
mod slash_commands;
pub(crate) use footer::CollaborationModeIndicator;
pub(crate) use list_selection_view::ColumnWidthMode;
pub(crate) use list_selection_view::SelectionViewParams;
pub(crate) use list_selection_view::SideContentWidth;
pub(crate) use list_selection_view::popup_content_width;
pub(crate) use list_selection_view::side_by_side_layout_widths;
mod feedback_view;
pub(crate) use feedback_view::FeedbackAudience;
pub(crate) use feedback_view::feedback_classification;
pub(crate) use feedback_view::feedback_disabled_params;
pub(crate) use feedback_view::feedback_selection_params;
pub(crate) use feedback_view::feedback_success_cell;
pub(crate) use feedback_view::feedback_upload_consent_params;
pub(crate) use skills_toggle_view::SkillsToggleItem;
pub(crate) use skills_toggle_view::SkillsToggleView;
pub(crate) use status_line_setup::StatusLineItem;
pub(crate) use status_line_setup::StatusLinePreviewData;
pub(crate) use status_line_setup::StatusLineSetupView;
pub(crate) use title_setup::TerminalTitleItem;
pub(crate) use title_setup::TerminalTitlePreviewData;
pub(crate) use title_setup::TerminalTitleSetupView;
mod paste_burst;
mod pending_input_preview;
mod pending_thread_approvals;
mod plugin_status_view;
pub mod popup_consts;
#[cfg(not(target_os = "linux"))]
mod recording_meter;
mod rendering;
mod scroll_state;
mod selection_popup_common;
mod textarea;
mod unified_exec_footer;
pub(crate) use command_popup::PluginCommandInvocation;
pub(crate) use feedback_view::FeedbackNoteView;
pub(crate) use plugin_status_view::PLUGIN_STATUS_VIEW_ID;
pub(crate) use plugin_status_view::PluginStatusDocument;
pub(crate) use plugin_status_view::PluginStatusView;

/// How long the "press again to quit" hint stays visible.
///
/// This is shared between:
/// - `ChatWidget`: arming the double-press quit shortcut.
/// - `BottomPane`/`ChatComposer`: rendering and expiring the footer hint.
///
/// Keeping a single value ensures Ctrl+C and Ctrl+D behave identically.
pub(crate) const QUIT_SHORTCUT_TIMEOUT: Duration = Duration::from_secs(1);

/// Whether Ctrl+C/Ctrl+D require a second press to quit.
///
/// A single accidental terminal control key must not close an active Praxis session.
pub(crate) const DOUBLE_PRESS_QUIT_SHORTCUT_ENABLED: bool = true;

/// The result of offering a cancellation key to a bottom-pane surface.
///
/// This is primarily used for Ctrl+C routing: active views can consume the key to dismiss
/// themselves, and the caller can decide what higher-level action (if any) to take when the key is
/// not handled locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Handled,
    NotHandled,
}

pub(crate) use chat_composer::ChatComposer;
pub(crate) use chat_composer::ChatComposerConfig;
pub(crate) use chat_composer::InputResult;
pub(crate) use prompt_args::parse_slash_name;

use crate::status_indicator_widget::StatusDetailsCapitalization;
use crate::status_indicator_widget::StatusIndicatorWidget;
pub(crate) use experimental_features_view::ExperimentalFeatureItem;
pub(crate) use experimental_features_view::ExperimentalFeaturesView;
pub(crate) use list_selection_view::SelectionAction;
pub(crate) use list_selection_view::SelectionItem;

/// Pane displayed in the lower half of the chat UI.
///
/// This is the owning container for the prompt input (`ChatComposer`) and the view stack
/// (`BottomPaneView`). It performs local input routing and renders time-based hints, while leaving
/// process-level decisions (quit, interrupt, shutdown) to `ChatWidget`.
pub(crate) struct BottomPane {
    /// Composer is retained even when a BottomPaneView is displayed so the
    /// input state is retained when the view is closed.
    composer: ChatComposer,

    /// Stack of views displayed instead of the composer (e.g. popups/modals).
    view_stack: Vec<Box<dyn BottomPaneView>>,

    app_event_tx: AppEventSender,
    frame_requester: FrameRequester,

    has_input_focus: bool,
    enhanced_keys_supported: bool,
    disable_paste_burst: bool,
    is_task_running: bool,
    esc_backtrack_hint: bool,
    animations_enabled: bool,

    /// Inline status indicator shown above the composer while a task is running.
    status: Option<StatusIndicatorWidget>,
    status_activity_message: Option<String>,
    status_footer_message: Option<String>,
    /// Unified exec session summary source.
    ///
    /// When a status row exists, this summary is mirrored inline in that row;
    /// when no status row exists, it renders as its own footer row.
    unified_exec_footer: UnifiedExecFooter,
    /// Preview of pending steers and queued drafts shown above the composer.
    pending_input_preview: PendingInputPreview,
    /// Inactive threads with pending approval requests.
    pending_thread_approvals: PendingThreadApprovals,
    context_window_percent: Option<i64>,
    context_window_used_tokens: Option<i64>,
}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) frame_requester: FrameRequester,
    pub(crate) has_input_focus: bool,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) placeholder_text: String,
    pub(crate) disable_paste_burst: bool,
    pub(crate) animations_enabled: bool,
    pub(crate) skills: Option<Vec<SkillMetadata>>,
}

include!("implementation/configuration.rs");
include!("implementation/input.rs");
include!("implementation/status.rs");
include!("implementation/views.rs");
include!("implementation/requests.rs");
include!("implementation/lifecycle.rs");

#[cfg(test)]
mod tests;
