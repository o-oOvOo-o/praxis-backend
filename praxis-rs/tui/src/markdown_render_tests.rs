use pretty_assertions::assert_eq;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use std::path::Path;

use crate::markdown_render::COLON_LOCATION_SUFFIX_RE;
use crate::markdown_render::HASH_LOCATION_SUFFIX_RE;
use crate::markdown_render::hyperlink_target_for_local_link_text;
use crate::markdown_render::render_markdown_text;
use crate::markdown_render::render_markdown_text_with_width_and_cwd;
use insta::assert_snapshot;

fn render_markdown_text_for_cwd(input: &str, cwd: &Path) -> Text<'static> {
    render_markdown_text_with_width_and_cwd(input, /*width*/ None, Some(cwd))
}

mod basics;
mod blockquotes;
mod code_blocks;
mod complex_html;
mod inline_links;
mod lists;
