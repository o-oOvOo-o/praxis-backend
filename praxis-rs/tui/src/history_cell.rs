//! Transcript/history cells for the Praxis TUI.
//!
//! A `HistoryCell` is the unit of display in the conversation UI, representing both committed
//! transcript entries and, transiently, an in-flight active cell that can mutate in place while
//! streaming.
//!
//! The transcript overlay (`Ctrl+T`) appends a cached live tail derived from the active cell, and
//! that cached tail is refreshed based on an active-cell cache key. Cells that change based on
//! elapsed time expose `transcript_animation_tick()`, and code that mutates the active cell in place
//! bumps the active-cell revision tracked by `ChatWidget`, so the cache key changes whenever the
//! rendered transcript output can change.

use crate::diff_render::create_diff_file_summary;
use crate::diff_render::create_patch_history_summary;
use crate::diff_render::display_path_for;
use crate::exec_cell::CommandOutput;
use crate::exec_cell::OutputLinesParams;
use crate::exec_cell::TOOL_CALL_MAX_LINES;
use crate::exec_cell::output_lines;
use crate::exec_cell::spinner;
use crate::exec_command::relativize_to_home;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::history_presentation::PatchCellId;
use crate::history_presentation::TranscriptCardId;
use crate::live_wrap::take_prefix_by_width;
use crate::markdown::append_markdown;
use crate::render::line_utils::line_to_static;
use crate::render::line_utils::prefix_lines;
use crate::render::line_utils::push_owned_lines;
use crate::render::renderable::Renderable;
use crate::style::proposed_plan_style;
use crate::style::user_message_style;
#[cfg(test)]
use crate::test_support::PathBufExt;
use crate::text_formatting::format_and_truncate_tool_result;
use crate::text_formatting::format_json_compact;
use crate::text_formatting::truncate_text;
use crate::tui_config::TuiRuntimeConfig;
use crate::update_action::UpdateAction;
use crate::version::PRAXIS_CLI_VERSION;
use crate::wrapping::RtOptions;
use crate::wrapping::adaptive_wrap_line;
use crate::wrapping::adaptive_wrap_lines;
use base64::Engine;
use chrono::DateTime;
use chrono::Utc;
use image::DynamicImage;
use image::ImageReader;
use praxis_app_gateway_protocol::McpServerStatus;
use praxis_config::types::McpServerTransportConfig;
use praxis_core::config::Config;
#[cfg(test)]
use praxis_core::mcp::McpManager;
#[cfg(test)]
use praxis_core::plugins::PluginsManager;
use praxis_core::web_search::web_search_detail;
#[cfg(test)]
use praxis_mcp::mcp::qualified_mcp_tool_name_prefix;
use praxis_otel::RuntimeMetricsSummary;
use praxis_protocol::ThreadId;
use praxis_protocol::account::PlanType;
#[cfg(test)]
use praxis_protocol::mcp::Resource;
#[cfg(test)]
use praxis_protocol::mcp::ResourceTemplate;
use praxis_protocol::models::WebSearchAction;
use praxis_protocol::models::local_image_label_text;
use praxis_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use praxis_protocol::plan_tool::PlanItemArg;
use praxis_protocol::plan_tool::StepStatus;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use praxis_protocol::protocol::FileChange;
use praxis_protocol::protocol::McpAuthStatus;
use praxis_protocol::protocol::McpInvocation;
use praxis_protocol::protocol::SessionConfiguredEvent;
use praxis_protocol::request_user_input::RequestUserInputAnswer;
use praxis_protocol::request_user_input::RequestUserInputQuestion;
use praxis_protocol::user_input::TextElement;
use praxis_utils_cli::format_env_display::format_env_display;
use ratatui::prelude::*;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use std::any::Any;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tracing::error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatLane {
    Assistant,
    User,
}

/// Represents an event to display in the conversation history. Returns its
/// `Vec<Line<'static>>` representation to make it easier to display in a
/// scrollable list.
/// A single renderable unit of conversation history.
///
/// Each cell produces logical `Line`s and reports how many viewport
/// rows those lines occupy at a given terminal width. The default
/// height implementations use `Paragraph::wrap` to account for lines
/// that overflow the viewport width (e.g. long URLs that are kept
/// intact by adaptive wrapping). Concrete types only need to override
/// heights when they apply additional layout logic beyond what
/// `Paragraph::line_count` captures.
pub(crate) trait HistoryCell: std::fmt::Debug + Send + Sync + Any {
    fn chat_lane(&self) -> ChatLane {
        ChatLane::Assistant
    }

    /// Returns the logical lines for the main chat viewport.
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;

    /// Returns the logical lines that should be committed into scrollback history.
    ///
    /// Most cells commit exactly what they display live. Cells with transient startup animations
    /// can override this to persist a stable resting frame instead of whichever animation frame
    /// happened to be visible when the cell was flushed.
    fn committed_display_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.display_lines(width)
    }

    /// Returns the number of viewport rows needed to render this cell.
    ///
    /// The default delegates to `Paragraph::line_count` with
    /// `Wrap { trim: false }`, which measures the actual row count after
    /// ratatui's viewport-level character wrapping. This is critical
    /// for lines containing URL-like tokens that are wider than the
    /// terminal — the logical line count would undercount.
    fn desired_height(&self, width: u16) -> u16 {
        Paragraph::new(Text::from(self.display_lines(width)))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }

    /// Returns lines for the transcript overlay (`Ctrl+T`).
    ///
    /// Defaults to `display_lines`. Override when the transcript
    /// representation differs (e.g. `ExecCell` shows all calls with
    /// `$`-prefixed commands and exit status).
    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.display_lines(width)
    }

    /// Returns the number of viewport rows for the transcript overlay.
    ///
    /// Uses the same `Paragraph::line_count` measurement as
    /// `desired_height`. Contains a workaround for a ratatui bug where
    /// a single whitespace-only line reports 2 rows instead of 1.
    fn desired_transcript_height(&self, width: u16) -> u16 {
        let lines = self.transcript_lines(width);
        // Workaround: ratatui's line_count returns 2 for a single
        // whitespace-only line. Clamp to 1 in that case.
        if let [line] = &lines[..]
            && line
                .spans
                .iter()
                .all(|s| s.content.chars().all(char::is_whitespace))
        {
            return 1;
        }

        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }

    fn is_stream_continuation(&self) -> bool {
        false
    }

    /// Returns a coarse "animation tick" when transcript output is time-dependent.
    ///
    /// The transcript overlay caches the rendered output of the in-flight active cell, so cells
    /// that include time-based UI (spinner, shimmer, etc.) should return a tick that changes over
    /// time to signal that the cached tail should be recomputed. Returning `None` means the
    /// transcript lines are stable, while returning `Some(tick)` during an in-flight animation
    /// allows the overlay to keep up with the main viewport.
    ///
    /// If a cell uses time-based visuals but always returns `None`, `Ctrl+T` can appear "frozen" on
    /// the first rendered frame even though the main viewport is animating.
    fn transcript_animation_tick(&self) -> Option<u64> {
        None
    }

    fn patch_cell_id(&self) -> Option<PatchCellId> {
        None
    }

    /// Returns clickable row targets for the cell, relative to the cell's rendered top row.
    fn mouse_targets(&self, _width: u16) -> Vec<HistoryCellMouseTarget> {
        Vec::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HistoryCellMouseAction {
    ResumeRecentThread {
        thread_id: ThreadId,
        thread_name: String,
    },
    ToggleTranscriptCard {
        card_id: TranscriptCardId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryCellMouseTarget {
    pub(crate) row_start: u16,
    pub(crate) row_end: u16,
    pub(crate) action: HistoryCellMouseAction,
}

impl HistoryCellMouseTarget {
    pub(crate) fn contains_row(&self, row: u16) -> bool {
        row >= self.row_start && row <= self.row_end
    }
}

pub(crate) const DIFF_TOGGLE_KEY_HINT: &str = "F8";

pub(crate) fn history_presentation_revision() -> u64 {
    crate::history_presentation::history_presentation_revision()
}

pub(crate) fn toggle_reasoning_expanded() -> bool {
    crate::history_presentation::toggle_reasoning_expanded()
}

pub(crate) fn toggle_tool_output_expanded() -> bool {
    crate::history_presentation::toggle_tool_output_expanded()
}

pub(crate) fn toggle_visible_diff_cells(ids: &[PatchCellId]) -> bool {
    crate::history_presentation::toggle_diff_cells(ids)
}

pub(crate) fn is_transcript_card_expanded(id: &TranscriptCardId) -> bool {
    crate::history_presentation::is_transcript_card_expanded(id)
}

pub(crate) fn toggle_transcript_card(id: TranscriptCardId) -> bool {
    crate::history_presentation::toggle_transcript_card(id)
}

impl Renderable for Box<dyn HistoryCell> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.display_lines(area.width);
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        let y = if area.height == 0 {
            0
        } else {
            let overflow = paragraph
                .line_count(area.width)
                .saturating_sub(usize::from(area.height));
            u16::try_from(overflow).unwrap_or(u16::MAX)
        };
        paragraph.scroll((y, 0)).render(area, buf);
    }
    fn desired_height(&self, width: u16) -> u16 {
        HistoryCell::desired_height(self.as_ref(), width)
    }
}

impl dyn HistoryCell {
    pub(crate) fn as_any(&self) -> &dyn Any {
        self
    }

    pub(crate) fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

mod approvals;
mod cards;
mod events;
mod exec;
mod mcp_tools;
mod patch;
mod session;
mod simple;
mod user;

pub(crate) use self::approvals::*;
pub(crate) use self::cards::*;
pub(crate) use self::events::*;
pub(crate) use self::exec::*;
pub(crate) use self::mcp_tools::*;
pub(crate) use self::patch::*;
pub(crate) use self::session::*;
pub(crate) use self::simple::*;
pub(crate) use self::user::*;

#[cfg(test)]
mod tests;
