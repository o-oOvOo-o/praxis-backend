//! The chat composer is the bottom-pane text input state machine.
//!
//! It is responsible for:
//!
//! - Editing the input buffer (a [`TextArea`]), including placeholder "elements" for attachments.
//! - Routing keys to the active popup (slash commands, file search, skill/apps mentions).
//! - Promoting typed slash commands into atomic elements when the command name is completed.
//! - Handling submit vs newline on Enter.
//! - Turning raw key streams into explicit paste operations on platforms where terminals
//!   don't provide reliable bracketed paste (notably Windows).
//!
//! # Key Event Routing
//!
//! Most key handling goes through [`ChatComposer::handle_key_event`], which dispatches to a
//! popup-specific handler if a popup is visible and otherwise to
//! [`ChatComposer::handle_key_event_without_popup`]. After every handled key, we call
//! [`ChatComposer::sync_popups`] so UI state follows the latest buffer/cursor.
//!
//! # History Navigation (↑/↓)
//!
//! The Up/Down history path is managed by [`ChatComposerHistory`]. It merges:
//!
//! - Persistent cross-session history (text-only; no element ranges or attachments).
//! - Local in-session history (full text + text elements + local/remote image attachments).
//!
//! When recalling a local entry, the composer rehydrates text elements and both attachment kinds
//! (local image paths + remote image URLs).
//! When recalling a persistent entry, only the text is restored.
//! Recalled entries move the cursor to end-of-line so repeated Up/Down presses keep shell-like
//! history traversal semantics instead of dropping to column 0.
//!
//! # Submission and Prompt Expansion
//!
//! `Enter` submits immediately. `Tab` requests queuing while a task is running; if no task is
//! running, `Tab` submits just like Enter so input is never dropped.
//! `Tab` does not submit when entering a `!` shell command.
//!
//! On submit/queue paths, the composer:
//!
//! - Expands pending paste placeholders so element ranges align with the final text.
//! - Trims whitespace and rebases text elements accordingly.
//! - Prunes local attached images so only placeholders that survive expansion are sent.
//! - Preserves remote image URLs as separate attachments even when text is empty.
//!
//! When these paths clear the visible textarea after a successful submit or slash-command
//! dispatch, they intentionally preserve the textarea kill buffer. That lets users `Ctrl+K` part
//! of a draft, perform a composer action such as changing reasoning level, and then `Ctrl+Y` the
//! killed text back into the now-empty draft.
//!
//! The numeric auto-submit path used by the slash popup performs the same pending-paste expansion
//! and attachment pruning, and clears pending paste state on success.
//! Slash commands with arguments (like `/plan` and `/review`) reuse the same preparation path so
//! pasted content and text elements are preserved when extracting args.
//!
//! # Remote Image Rows (Up/Down/Delete)
//!
//! Remote image URLs are rendered as non-editable `[Image #N]` rows above the textarea (inside the
//! same composer block). These rows represent image attachments rehydrated from app-gateway/backtrack
//! history; TUI users can remove them, but cannot type into that row region.
//!
//! Keyboard behavior:
//!
//! - `Up` at textarea cursor `0` enters remote-row selection at the last remote image.
//! - `Up`/`Down` move selection between remote rows.
//! - `Down` on the last row clears selection and returns control to the textarea.
//! - `Delete`/`Backspace` remove the selected remote image row.
//!
//! Placeholder numbering is unified across remote and local images:
//!
//! - Remote rows occupy `[Image #1]..[Image #M]`.
//! - Local placeholders are offset after that range (`[Image #M+1]..`).
//! - Deleting a remote row relabels local placeholders to keep numbering contiguous.
//!
//! # Non-bracketed Paste Bursts
//!
//! On some terminals (especially on Windows), pastes arrive as a rapid sequence of
//! `KeyCode::Char` and `KeyCode::Enter` key events instead of a single paste event.
//!
//! To avoid misinterpreting these bursts as real typing (and to prevent transient UI effects like
//! shortcut overlays toggling on a pasted `?`), we feed "plain" character events into
//! [`PasteBurst`](super::paste_burst::PasteBurst), which buffers bursts and later flushes them
//! through [`ChatComposer::handle_paste`].
//!
//! The burst detector intentionally treats ASCII and non-ASCII differently:
//!
//! - ASCII: we briefly hold the first fast char (flicker suppression) until we know whether the
//!   stream is paste-like.
//! - non-ASCII: we do not hold the first char (IME input would feel dropped), but we still allow
//!   burst detection for actual paste streams.
//!
//! The burst detector can also be disabled (`disable_paste_burst`), which bypasses the state
//! machine and treats the key stream as normal typing. When toggling from enabled → disabled, the
//! composer flushes/clears any in-flight burst state so it cannot leak into subsequent input.
//!
//! For the detailed burst state machine, see `praxis-rs/tui/src/bottom_pane/paste_burst.rs`.
//! For a narrative overview of the combined state machine, see `docs/tui-chat-composer.md`.
//!
//! # PasteBurst Integration Points
//!
//! The burst detector is consulted in a few specific places:
//!
//! - [`ChatComposer::handle_input_basic`]: flushes any due burst first, then intercepts plain char
//!   input to either buffer it or insert normally.
//! - [`ChatComposer::handle_non_ascii_char`]: handles the non-ASCII/IME path without holding the
//!   first char, while still allowing paste detection via retro-capture.
//! - [`ChatComposer::flush_paste_burst_if_due`]/[`ChatComposer::handle_paste_burst_flush`]: called
//!   from UI ticks to turn a pending burst into either an explicit paste (`handle_paste`) or a
//!   normal typed character.
//!
//! # Input Disabled Mode
//!
//! The composer can be temporarily read-only (`input_enabled = false`). In that mode it ignores
//! edits and renders a placeholder prompt instead of the editable textarea. This is part of the
//! overall state machine, since it affects which transitions are even possible from a given UI
//! state.
//!
use crate::bottom_pane::footer::mode_indicator_line;
use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::key_hint::has_ctrl_or_alt;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::ui_consts::FOOTER_INDENT_COLS;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use praxis_app_core::thread_commands::ExternalThreadCommandIntent;
use praxis_app_core::thread_commands::parse_external_thread_command;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;

use super::chat_composer_history::ChatComposerHistory;
use super::chat_composer_history::HistoryEntry;
use super::command_popup::CommandItem;
use super::command_popup::CommandPopup;
use super::command_popup::CommandPopupFlags;
use super::command_popup::PluginCommandInvocation;
use super::command_popup::PluginCommandItem;
use super::file_search_popup::FileSearchPopup;
use super::footer::CollaborationModeIndicator;
use super::footer::FooterMode;
use super::footer::FooterProps;
use super::footer::SummaryLeft;
use super::footer::can_show_left_with_context;
use super::footer::context_window_line;
use super::footer::esc_hint_mode;
use super::footer::footer_height;
use super::footer::footer_hint_items_width;
use super::footer::footer_line_width;
use super::footer::footer_show_queue_hint;
use super::footer::footer_show_shortcuts_hint;
use super::footer::inset_footer_hint_area;
use super::footer::max_left_width_for_right;
use super::footer::passive_footer_status_line;
use super::footer::render_context_right;
use super::footer::render_footer_from_props;
use super::footer::render_footer_hint_items;
use super::footer::render_footer_line;
use super::footer::reset_mode_after_activity;
use super::footer::single_line_footer_layout;
use super::footer::toggle_shortcut_mode;
use super::footer::uses_passive_footer_status_layout;
use super::paste_burst::CharDecision;
use super::paste_burst::PasteBurst;
use super::skill_popup::MentionItem;
use super::skill_popup::SkillPopup;
use super::slash_commands;
use super::slash_commands::BuiltinCommandFlags;
use crate::bottom_pane::paste_burst::FlushResult;
use crate::bottom_pane::prompt_args::parse_slash_name;
use crate::render::Insets;
use crate::render::RectExt;
use crate::render::renderable::Renderable;
use crate::slash_command::SlashCommand;
use crate::surface::SurfaceTheme;
use praxis_protocol::models::local_image_label_text;
use praxis_protocol::user_input::ByteRange;
use praxis_protocol::user_input::MAX_USER_INPUT_TEXT_CHARS;
use praxis_protocol::user_input::TextElement;

use crate::app_event::AppEvent;
use crate::app_event::ConnectorsSnapshot;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::LocalImageAttachment;
use crate::bottom_pane::MentionBinding;
use crate::bottom_pane::textarea::TextArea;
use crate::bottom_pane::textarea::TextAreaState;
use crate::clipboard_paste::normalize_pasted_path;
use crate::clipboard_paste::pasted_image_format;
use crate::history_cell;
use crate::tui::FrameRequester;
use crate::ui_consts::LIVE_PREFIX_COLS;
use praxis_chatgpt::connectors;
use praxis_chatgpt::connectors::AppInfo;
use praxis_core::plugins::PluginCapabilitySummary;
use praxis_core::skills::model::SkillMetadata;
use praxis_file_search::FileMatch;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
/// If the pasted content exceeds this number of characters, replace it with a
/// placeholder in the UI.
const LARGE_PASTE_CHAR_THRESHOLD: usize = 1000;

fn user_input_too_large_message(actual_chars: usize) -> String {
    format!(
        "Message exceeds the maximum length of {MAX_USER_INPUT_TEXT_CHARS} characters ({actual_chars} provided)."
    )
}

/// Result returned when the user interacts with the text area.
#[derive(Debug, PartialEq)]
pub enum InputResult {
    Submitted {
        text: String,
        text_elements: Vec<TextElement>,
    },
    Queued {
        text: String,
        text_elements: Vec<TextElement>,
    },
    Command(SlashCommand),
    CommandWithArgs(SlashCommand, String, Vec<TextElement>),
    PluginCommand(PluginCommandInvocation),
    ThreadCommand(ExternalThreadCommandIntent),
    None,
}

#[derive(Clone, Debug, PartialEq)]
struct AttachedImage {
    placeholder: String,
    path: PathBuf,
}

/// Feature flags for reusing the chat composer in other bottom-pane surfaces.
///
/// The default keeps today's behavior intact. Other call sites can opt out of
/// specific behaviors by constructing a config with those flags set to `false`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChatComposerConfig {
    /// Whether command/file/skill popups are allowed to appear.
    pub(crate) popups_enabled: bool,
    /// Whether `/...` input is parsed and dispatched as slash commands.
    pub(crate) slash_commands_enabled: bool,
    /// Whether pasting a file path can attach local images.
    pub(crate) image_paste_enabled: bool,
}

impl Default for ChatComposerConfig {
    fn default() -> Self {
        Self {
            popups_enabled: true,
            slash_commands_enabled: true,
            image_paste_enabled: true,
        }
    }
}

impl ChatComposerConfig {
    /// A minimal preset for plain-text inputs embedded in other surfaces.
    ///
    /// This disables popups, slash commands, and image-path attachment behavior
    /// so the composer behaves like a simple notes field.
    pub(crate) const fn plain_text() -> Self {
        Self {
            popups_enabled: false,
            slash_commands_enabled: false,
            image_paste_enabled: false,
        }
    }
}

pub(crate) struct ChatComposer {
    textarea: TextArea,
    textarea_state: RefCell<TextAreaState>,
    active_popup: ActivePopup,
    app_event_tx: AppEventSender,
    history: ChatComposerHistory,
    quit_shortcut_expires_at: Option<Instant>,
    quit_shortcut_key: KeyBinding,
    esc_backtrack_hint: bool,
    use_shift_enter_hint: bool,
    dismissed_file_popup_token: Option<String>,
    current_file_query: Option<String>,
    pending_pastes: Vec<(String, String)>,
    large_paste_counters: HashMap<usize, usize>,
    has_focus: bool,
    frame_requester: Option<FrameRequester>,
    /// Invariant: attached images are labeled in vec order as
    /// `[Image #M+1]..[Image #N]`, where `M` is the number of remote images.
    attached_images: Vec<AttachedImage>,
    placeholder_text: String,
    is_task_running: bool,
    /// When false, the composer is temporarily read-only (e.g. during sandbox setup).
    input_enabled: bool,
    input_disabled_placeholder: Option<String>,
    /// Non-bracketed paste burst tracker (see `bottom_pane/paste_burst.rs`).
    paste_burst: PasteBurst,
    // When true, disables paste-burst logic and inserts characters immediately.
    disable_paste_burst: bool,
    footer_mode: FooterMode,
    footer_hint_override: Option<Vec<(String, String)>>,
    remote_image_urls: Vec<String>,
    /// Tracks keyboard selection for the remote-image rows so Up/Down + Delete/Backspace
    /// can highlight and remove remote attachments from the composer UI.
    selected_remote_image_index: Option<usize>,
    footer_flash: Option<FooterFlash>,
    context_window_percent: Option<i64>,
    // Monotonically increasing identifier for textarea elements we insert.
    #[cfg(not(target_os = "linux"))]
    next_element_id: u64,
    context_window_used_tokens: Option<i64>,
    skills: Option<Vec<SkillMetadata>>,
    plugins: Option<Vec<PluginCapabilitySummary>>,
    connectors_snapshot: Option<ConnectorsSnapshot>,
    dismissed_mention_popup_token: Option<String>,
    mention_bindings: HashMap<u64, ComposerMentionBinding>,
    recent_submission_mention_bindings: Vec<MentionBinding>,
    collaboration_modes_enabled: bool,
    config: ChatComposerConfig,
    collaboration_mode_indicator: Option<CollaborationModeIndicator>,
    connectors_enabled: bool,
    plugins_command_enabled: bool,
    fast_command_enabled: bool,
    personality_command_enabled: bool,
    realtime_conversation_enabled: bool,
    audio_device_selection_enabled: bool,
    windows_degraded_sandbox_active: bool,
    status_line_value: Option<Line<'static>>,
    status_line_enabled: bool,
    // Agent label injected into the footer's contextual row when multi-agent mode is active.
    active_agent_label: Option<String>,
    footer_right_badge: Option<Line<'static>>,
    surface_theme: SurfaceTheme,
}

#[derive(Clone, Debug)]
struct FooterFlash {
    line: Line<'static>,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
struct ComposerMentionBinding {
    mention: String,
    path: String,
}

/// Popup state – at most one can be visible at any time.
enum ActivePopup {
    None,
    Command(CommandPopup),
    File(FileSearchPopup),
    Skill(SkillPopup),
}

const FOOTER_SPACING_HEIGHT: u16 = 0;

mod popups;

#[cfg(not(target_os = "linux"))]
mod recording;
mod remote_and_sync;
mod render;
mod settings;
mod state_and_input;
mod submission;

#[cfg(test)]
mod tests;

fn skill_display_name(skill: &SkillMetadata) -> &str {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .unwrap_or(&skill.name)
}

fn skill_description(skill: &SkillMetadata) -> Option<String> {
    let description = skill
        .interface
        .as_ref()
        .and_then(|interface| interface.short_description.as_deref())
        .or(skill.short_description.as_deref())
        .unwrap_or(&skill.description);
    let trimmed = description.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn is_mention_name_char(byte: u8) -> bool {
    matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
}

fn find_next_mention_token_range(text: &str, token: &str, from: usize) -> Option<Range<usize>> {
    if token.is_empty() || from >= text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let token_bytes = token.as_bytes();
    let mut index = from;

    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }

        let end = index.saturating_add(token_bytes.len());
        if end > bytes.len() {
            return None;
        }
        if &bytes[index..end] != token_bytes {
            index += 1;
            continue;
        }

        if bytes
            .get(end)
            .is_none_or(|byte| !is_mention_name_char(*byte))
        {
            return Some(index..end);
        }

        index = end;
    }

    None
}
