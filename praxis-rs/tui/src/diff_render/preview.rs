use super::*;

#[derive(Default)]
pub(super) struct PreviewRenderStats {
    pub(super) shown_source_lines: usize,
    pub(super) total_source_lines: usize,
    pub(super) truncated: bool,
}

pub(super) fn render_change_preview(
    change: &FileChange,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    max_render_lines: usize,
    max_source_lines: usize,
) -> PreviewRenderStats {
    let style_context = current_diff_render_style_context();
    match change {
        FileChange::Add { content } => render_add_delete_preview(
            content,
            DiffLineType::Insert,
            out,
            width,
            max_render_lines,
            max_source_lines,
            style_context,
        ),
        FileChange::Delete { content } => render_add_delete_preview(
            content,
            DiffLineType::Delete,
            out,
            width,
            max_render_lines,
            max_source_lines,
            style_context,
        ),
        FileChange::Update { unified_diff, .. } => render_update_preview(
            unified_diff,
            out,
            width,
            max_render_lines,
            max_source_lines,
            style_context,
        ),
    }
}

fn render_add_delete_preview(
    content: &str,
    kind: DiffLineType,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    max_render_lines: usize,
    max_source_lines: usize,
    style_context: DiffRenderStyleContext,
) -> PreviewRenderStats {
    let total_source_lines = content.lines().count();
    let line_number_width = line_number_width(total_source_lines);
    let mut stats = PreviewRenderStats {
        total_source_lines,
        ..Default::default()
    };

    for (idx, raw) in content.lines().enumerate() {
        if out.len() >= max_render_lines || stats.shown_source_lines >= max_source_lines {
            stats.truncated = true;
            break;
        }
        stats.shown_source_lines += 1;
        push_preview_line(
            out,
            idx + 1,
            kind,
            raw,
            width,
            line_number_width,
            max_render_lines,
            style_context,
        );
    }

    if stats.shown_source_lines < stats.total_source_lines {
        stats.truncated = true;
    }
    stats
}

fn render_update_preview(
    unified_diff: &str,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    max_render_lines: usize,
    max_source_lines: usize,
    style_context: DiffRenderStyleContext,
) -> PreviewRenderStats {
    let Ok(patch) = diffy::Patch::from_str(unified_diff) else {
        return render_raw_unified_preview(
            unified_diff,
            out,
            width,
            max_render_lines,
            max_source_lines,
            style_context,
        );
    };

    let mut stats = PreviewRenderStats {
        total_source_lines: patch.hunks().iter().map(|hunk| hunk.lines().len()).sum(),
        ..Default::default()
    };
    let mut max_line_number = 0;
    for hunk in patch.hunks() {
        max_line_number = max_line_number.max(hunk.old_range().end());
        max_line_number = max_line_number.max(hunk.new_range().end());
    }
    let line_number_width = line_number_width(max_line_number);

    let mut is_first_hunk = true;
    'hunks: for hunk in patch.hunks() {
        if !is_first_hunk {
            out.push(RtLine::from(vec![
                RtSpan::styled(
                    format!("{:width$} ", "", width = line_number_width.max(1)),
                    style_gutter_for(
                        DiffLineType::Context,
                        style_context.theme,
                        style_context.color_level,
                    ),
                ),
                "...".dim(),
            ]));
            if out.len() >= max_render_lines {
                stats.truncated = true;
                break;
            }
        }
        is_first_hunk = false;

        let mut old_ln = hunk.old_range().start();
        let mut new_ln = hunk.new_range().start();
        for line in hunk.lines() {
            if out.len() >= max_render_lines || stats.shown_source_lines >= max_source_lines {
                stats.truncated = true;
                break 'hunks;
            }
            stats.shown_source_lines += 1;
            match line {
                diffy::Line::Insert(text) => {
                    push_preview_line(
                        out,
                        new_ln,
                        DiffLineType::Insert,
                        text.trim_end_matches('\n'),
                        width,
                        line_number_width,
                        max_render_lines,
                        style_context,
                    );
                    new_ln += 1;
                }
                diffy::Line::Delete(text) => {
                    push_preview_line(
                        out,
                        old_ln,
                        DiffLineType::Delete,
                        text.trim_end_matches('\n'),
                        width,
                        line_number_width,
                        max_render_lines,
                        style_context,
                    );
                    old_ln += 1;
                }
                diffy::Line::Context(text) => {
                    push_preview_line(
                        out,
                        new_ln,
                        DiffLineType::Context,
                        text.trim_end_matches('\n'),
                        width,
                        line_number_width,
                        max_render_lines,
                        style_context,
                    );
                    old_ln += 1;
                    new_ln += 1;
                }
            }
        }
    }

    if stats.shown_source_lines < stats.total_source_lines {
        stats.truncated = true;
    }
    stats
}

fn render_raw_unified_preview(
    unified_diff: &str,
    out: &mut Vec<RtLine<'static>>,
    width: usize,
    max_render_lines: usize,
    max_source_lines: usize,
    style_context: DiffRenderStyleContext,
) -> PreviewRenderStats {
    let total_source_lines = unified_diff.lines().count();
    let mut stats = PreviewRenderStats {
        total_source_lines,
        ..Default::default()
    };
    let line_number_width = line_number_width(total_source_lines);
    for (idx, raw) in unified_diff.lines().enumerate() {
        if out.len() >= max_render_lines || stats.shown_source_lines >= max_source_lines {
            stats.truncated = true;
            break;
        }
        stats.shown_source_lines += 1;
        let kind = if raw.starts_with('+') {
            DiffLineType::Insert
        } else if raw.starts_with('-') {
            DiffLineType::Delete
        } else {
            DiffLineType::Context
        };
        push_preview_line(
            out,
            idx + 1,
            kind,
            raw,
            width,
            line_number_width,
            max_render_lines,
            style_context,
        );
    }
    if stats.shown_source_lines < stats.total_source_lines {
        stats.truncated = true;
    }
    stats
}

fn push_preview_line(
    out: &mut Vec<RtLine<'static>>,
    line_number: usize,
    kind: DiffLineType,
    raw: &str,
    width: usize,
    line_number_width: usize,
    max_render_lines: usize,
    style_context: DiffRenderStyleContext,
) {
    let text = truncate_preview_line(raw);
    out.extend(push_wrapped_diff_line_inner_with_theme_and_color_level(
        line_number,
        kind,
        text.as_str(),
        width,
        line_number_width,
        /*syntax_spans*/ None,
        style_context.theme,
        style_context.color_level,
        style_context.diff_backgrounds,
    ));
    if out.len() > max_render_lines {
        out.truncate(max_render_lines);
    }
}

fn truncate_preview_line(raw: &str) -> String {
    let mut text = String::new();
    for (idx, ch) in raw.chars().enumerate() {
        if idx >= PATCH_HISTORY_PREVIEW_MAX_LINE_CHARS {
            text.push_str("...");
            return text;
        }
        text.push(ch);
    }
    text
}

/// Format a path for display relative to the current working directory when
/// possible, keeping output stable in jj/no-`.git` workspaces (e.g. image
/// tool calls should show `example.png` instead of an absolute path).
pub(crate) fn display_path_for(path: &Path, cwd: &Path) -> String {
    if path.is_relative() {
        return path.display().to_string();
    }

    if let Ok(stripped) = path.strip_prefix(cwd) {
        return stripped.display().to_string();
    }

    let path_in_same_repo = match (get_git_repo_root(cwd), get_git_repo_root(path)) {
        (Some(cwd_repo), Some(path_repo)) => cwd_repo == path_repo,
        _ => false,
    };
    let chosen = if path_in_same_repo {
        pathdiff::diff_paths(path, cwd).unwrap_or_else(|| path.to_path_buf())
    } else {
        relativize_to_home(path)
            .map(|p| PathBuf::from_iter([Path::new("~"), p.as_path()]))
            .unwrap_or_else(|| path.to_path_buf())
    };
    chosen.display().to_string()
}

pub(crate) fn calculate_add_remove_from_diff(diff: &str) -> (usize, usize) {
    if let Ok(patch) = diffy::Patch::from_str(diff) {
        patch
            .hunks()
            .iter()
            .flat_map(Hunk::lines)
            .fold((0, 0), |(a, d), l| match l {
                diffy::Line::Insert(_) => (a + 1, d),
                diffy::Line::Delete(_) => (a, d + 1),
                diffy::Line::Context(_) => (a, d),
            })
    } else {
        // For unparsable diffs, return 0 for both counts.
        (0, 0)
    }
}

/// Render a single plain-text (non-syntax-highlighted) diff line, wrapped to
/// `width` columns, using a pre-computed [`DiffRenderStyleContext`].
///
/// This is the convenience entry point used by the theme picker preview and
/// any caller that does not have syntax spans.  Delegates to the inner
/// rendering core with `syntax_spans = None`.
pub(crate) fn push_wrapped_diff_line_with_style_context(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    style_context: DiffRenderStyleContext,
) -> Vec<RtLine<'static>> {
    push_wrapped_diff_line_inner_with_theme_and_color_level(
        line_number,
        kind,
        text,
        width,
        line_number_width,
        /*syntax_spans*/ None,
        style_context.theme,
        style_context.color_level,
        style_context.diff_backgrounds,
    )
}

/// Render a syntax-highlighted diff line, wrapped to `width` columns, using
/// a pre-computed [`DiffRenderStyleContext`].
///
/// Like [`push_wrapped_diff_line_with_style_context`] but overlays
/// `syntax_spans` (from [`highlight_code_to_styled_spans`]) onto the diff
/// coloring.  Delete lines receive a `DIM` modifier so syntax colors do not
/// overpower the removal cue.
pub(crate) fn push_wrapped_diff_line_with_syntax_and_style_context(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    width: usize,
    line_number_width: usize,
    syntax_spans: &[RtSpan<'static>],
    style_context: DiffRenderStyleContext,
) -> Vec<RtLine<'static>> {
    push_wrapped_diff_line_inner_with_theme_and_color_level(
        line_number,
        kind,
        text,
        width,
        line_number_width,
        Some(syntax_spans),
        style_context.theme,
        style_context.color_level,
        style_context.diff_backgrounds,
    )
}
