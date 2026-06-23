//! Renders unified diffs with line numbers, gutter signs, and optional syntax
//! highlighting.
//!
//! Each `FileChange` variant (Add / Delete / Update) is rendered as a block of
//! diff lines, each prefixed by a right-aligned line number, a gutter sign
//! (`+` / `-` / ` `), and the content text.  When a recognized file extension
//! is present, the content text is syntax-highlighted using
//! [`crate::render::highlight`].
//!
//! **Theme-aware styling:** diff backgrounds adapt to the terminal's
//! background lightness via [`DiffTheme`].  Dark terminals get muted tints
//! (`#212922` green, `#3C170F` red); light terminals get GitHub-style pastels
//! with distinct gutter backgrounds for contrast. The renderer uses fixed
//! palettes for truecolor / 256-color / 16-color terminals so add/delete lines
//! remain visually distinct even when quantizing to limited palettes.
//!
//! **Syntax-theme scope backgrounds:** when the active syntax theme defines
//! background colors for `markup.inserted` / `markup.deleted` (or fallback
//! `diff.inserted` / `diff.deleted`) scopes, those colors override the
//! hardcoded palette for rich color levels.  ANSI-16 mode always uses
//! foreground-only styling regardless of theme scope backgrounds.
//!
//! **Highlighting strategy for `Update` diffs:** the renderer highlights each
//! hunk as a single concatenated block rather than line-by-line.  This
//! preserves syntect's parser state across consecutive lines within a hunk
//! (important for multi-line strings, block comments, etc.).  Cross-hunk state
//! is intentionally *not* preserved because hunks are visually separated and
//! re-synchronize at context boundaries anyway.
//!
//! **Wrapping:** long lines are hard-wrapped at the available column width.
//! Syntax-highlighted spans are split at character boundaries with styles
//! preserved across the split so that no color information is lost.

use diffy::Hunk;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line as RtLine;
use ratatui::text::Span as RtSpan;
use ratatui::widgets::Paragraph;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use unicode_width::UnicodeWidthChar;

/// Display width of a tab character in columns.
const TAB_WIDTH: usize = 4;
const PATCH_HISTORY_PREVIEW_MAX_RENDER_LINES: usize = 96;
const PATCH_HISTORY_PREVIEW_MAX_SOURCE_LINES_PER_FILE: usize = 48;
const PATCH_HISTORY_PREVIEW_MAX_LINE_CHARS: usize = 220;

// -- Diff background palette --------------------------------------------------
//
// Dark-theme tints are subtle enough to avoid clashing with syntax colors.
// Light-theme values match GitHub's diff colors for familiarity.  The gutter
// (line-number column) uses slightly more saturated variants on light
// backgrounds so the numbers remain readable against the pastel line background.
// Truecolor palette.
const DARK_TC_ADD_LINE_BG_RGB: (u8, u8, u8) = (33, 58, 43); // #213A2B
const DARK_TC_DEL_LINE_BG_RGB: (u8, u8, u8) = (74, 34, 29); // #4A221D
const LIGHT_TC_ADD_LINE_BG_RGB: (u8, u8, u8) = (218, 251, 225); // #dafbe1
const LIGHT_TC_DEL_LINE_BG_RGB: (u8, u8, u8) = (255, 235, 233); // #ffebe9
const LIGHT_TC_ADD_NUM_BG_RGB: (u8, u8, u8) = (172, 238, 187); // #aceebb
const LIGHT_TC_DEL_NUM_BG_RGB: (u8, u8, u8) = (255, 206, 203); // #ffcecb
const LIGHT_TC_GUTTER_FG_RGB: (u8, u8, u8) = (31, 35, 40); // #1f2328

// 256-color palette.
const DARK_256_ADD_LINE_BG_IDX: u8 = 22;
const DARK_256_DEL_LINE_BG_IDX: u8 = 52;
const LIGHT_256_ADD_LINE_BG_IDX: u8 = 194;
const LIGHT_256_DEL_LINE_BG_IDX: u8 = 224;
const LIGHT_256_ADD_NUM_BG_IDX: u8 = 157;
const LIGHT_256_DEL_NUM_BG_IDX: u8 = 217;
const LIGHT_256_GUTTER_FG_IDX: u8 = 236;

use crate::color::is_light;
use crate::color::perceptual_distance;
use crate::exec_command::relativize_to_home;
use crate::render::Insets;
use crate::render::highlight::DiffScopeBackgroundRgbs;
use crate::render::highlight::diff_scope_background_rgbs;
use crate::render::highlight::exceeds_highlight_limits;
use crate::render::highlight::highlight_code_to_styled_spans;
use crate::render::line_utils::prefix_lines;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::InsetRenderable;
use crate::render::renderable::Renderable;
use crate::terminal_palette::StdoutColorLevel;
use crate::terminal_palette::XTERM_COLORS;
use crate::terminal_palette::default_bg;
use crate::terminal_palette::indexed_color;
use crate::terminal_palette::rgb_color;
use crate::terminal_palette::stdout_color_level;
use praxis_git_utils::get_git_repo_root;
use praxis_protocol::protocol::FileChange;
use praxis_terminal_detection::TerminalName;
use praxis_terminal_detection::terminal_info;

/// Classifies a diff line for gutter sign rendering and style selection.
///
/// `Insert` renders with a `+` sign and green text, `Delete` with `-` and red
/// text (plus dim overlay when syntax-highlighted), and `Context` with a space
/// and default styling.
#[derive(Clone, Copy)]
pub(crate) enum DiffLineType {
    Insert,
    Delete,
    Context,
}

mod preview;
mod render_lines;
mod summary;
mod theme;
mod wrapping;

pub(crate) use self::preview::*;
pub(crate) use self::render_lines::*;
pub(crate) use self::summary::*;
pub(crate) use self::theme::*;
pub(crate) use self::wrapping::*;

#[cfg(test)]
mod tests;
