use super::*;

pub struct DiffSummary {
    changes: HashMap<PathBuf, FileChange>,
    cwd: PathBuf,
}

impl DiffSummary {
    pub fn new(changes: HashMap<PathBuf, FileChange>, cwd: PathBuf) -> Self {
        Self { changes, cwd }
    }
}

impl Renderable for FileChange {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut lines = vec![];
        render_change(self, &mut lines, area.width as usize, /*lang*/ None);
        Paragraph::new(lines).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let mut lines = vec![];
        render_change(self, &mut lines, width as usize, /*lang*/ None);
        lines.len() as u16
    }
}

impl From<DiffSummary> for Box<dyn Renderable> {
    fn from(val: DiffSummary) -> Self {
        let mut rows: Vec<Box<dyn Renderable>> = vec![];

        for (i, row) in collect_rows(&val.changes).into_iter().enumerate() {
            if i > 0 {
                rows.push(Box::new(RtLine::from("")));
            }
            let mut path = RtLine::from(display_path_for(&row.path, &val.cwd));
            path.push_span(" ");
            path.extend(render_line_count_summary(row.added, row.removed));
            rows.push(Box::new(path));
            rows.push(Box::new(RtLine::from("")));
            rows.push(Box::new(InsetRenderable::new(
                Box::new(row.change) as Box<dyn Renderable>,
                Insets::tlbr(
                    /*top*/ 0, /*left*/ 2, /*bottom*/ 0, /*right*/ 0,
                ),
            )));
        }

        Box::new(ColumnRenderable::with(rows))
    }
}

#[cfg(test)]
pub(crate) fn create_diff_summary(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
    wrap_cols: usize,
) -> Vec<RtLine<'static>> {
    let rows = collect_rows(changes);
    render_changes_block(rows, wrap_cols, cwd)
}

pub(crate) fn create_patch_history_summary(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
    wrap_cols: usize,
) -> Vec<RtLine<'static>> {
    let rows = collect_rows(changes);
    render_patch_history_block(rows, wrap_cols, cwd)
}

pub(crate) fn create_diff_file_summary(
    changes: &HashMap<PathBuf, FileChange>,
    cwd: &Path,
) -> Vec<RtLine<'static>> {
    let rows = collect_rows(changes);
    render_change_header(rows, cwd)
}

// Shared row for per-file presentation
#[derive(Clone)]
struct Row {
    #[allow(dead_code)]
    path: PathBuf,
    move_path: Option<PathBuf>,
    added: usize,
    removed: usize,
    change: FileChange,
}

fn collect_rows(changes: &HashMap<PathBuf, FileChange>) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for (path, change) in changes.iter() {
        let (added, removed) = match change {
            FileChange::Add { content } => (content.lines().count(), 0),
            FileChange::Delete { content } => (0, content.lines().count()),
            FileChange::Update { unified_diff, .. } => calculate_add_remove_from_diff(unified_diff),
        };
        let move_path = match change {
            FileChange::Update {
                move_path: Some(new),
                ..
            } => Some(new.clone()),
            _ => None,
        };
        rows.push(Row {
            path: path.clone(),
            move_path,
            added,
            removed,
            change: change.clone(),
        });
    }
    rows.sort_by_key(|r| r.path.clone());
    rows
}

fn render_line_count_summary(added: usize, removed: usize) -> Vec<RtSpan<'static>> {
    let mut spans = Vec::new();
    spans.push("(".into());
    spans.push(format!("+{added}").green());
    spans.push(" ".into());
    spans.push(format!("-{removed}").red());
    spans.push(")".into());
    spans
}

#[cfg(test)]
fn render_changes_block(rows: Vec<Row>, wrap_cols: usize, cwd: &Path) -> Vec<RtLine<'static>> {
    let file_count = rows.len();
    let mut out = render_change_header(rows.clone(), cwd);

    for (idx, r) in rows.into_iter().enumerate() {
        // Insert a blank separator between file chunks (except before the first)
        if idx > 0 {
            out.push("".into());
        }
        // File header line (skip when single-file header already shows the name)
        let skip_file_header = file_count == 1;
        if !skip_file_header {
            let mut header: Vec<RtSpan<'static>> = Vec::new();
            header.push("  └ ".dim());
            header.extend(render_row_path(&r, cwd));
            header.push(" ".into());
            header.extend(render_line_count_summary(r.added, r.removed));
            out.push(RtLine::from(header));
        }

        // For renames, use the destination extension for highlighting — the
        // diff content reflects the new file, not the old one.
        let lang_path = r.move_path.as_deref().unwrap_or(&r.path);
        let lang = detect_lang_for_path(lang_path);
        let mut lines = vec![];
        render_change(&r.change, &mut lines, wrap_cols - 4, lang.as_deref());
        out.extend(prefix_lines(lines, "    ".into(), "    ".into()));
    }

    out
}

fn render_patch_history_block(
    rows: Vec<Row>,
    wrap_cols: usize,
    cwd: &Path,
) -> Vec<RtLine<'static>> {
    let file_count = rows.len();
    let mut out = render_change_header(rows.clone(), cwd);
    if rows.is_empty() {
        return out;
    }

    out.push(RtLine::from(vec![
        "  └ ".dim(),
        format!(
            "preview limited to {PATCH_HISTORY_PREVIEW_MAX_RENDER_LINES} rendered lines; full diff omitted from chat history"
        )
        .dim(),
    ]));

    let mut remaining_render_lines = PATCH_HISTORY_PREVIEW_MAX_RENDER_LINES;
    for (idx, r) in rows.into_iter().enumerate() {
        if idx > 0 {
            out.push("".into());
        }

        let mut header: Vec<RtSpan<'static>> = Vec::new();
        header.push(if file_count == 1 {
            "  └ ".dim()
        } else {
            "  • ".dim()
        });
        header.extend(render_row_path(&r, cwd));
        header.push(" ".into());
        header.extend(render_line_count_summary(r.added, r.removed));
        out.push(RtLine::from(header));

        if remaining_render_lines == 0 {
            out.push(RtLine::from(vec![
                "    ... ".dim(),
                "more diff omitted".dim(),
            ]));
            continue;
        }

        let preview_width = wrap_cols.saturating_sub(4).max(20);
        let mut preview = Vec::new();
        let stats = render_change_preview(
            &r.change,
            &mut preview,
            preview_width,
            remaining_render_lines,
            PATCH_HISTORY_PREVIEW_MAX_SOURCE_LINES_PER_FILE,
        );
        let used = preview.len();
        remaining_render_lines = remaining_render_lines.saturating_sub(used);
        out.extend(prefix_lines(preview, "    ".into(), "    ".into()));

        if stats.truncated {
            out.push(RtLine::from(vec![
                "    ... ".dim(),
                format!(
                    "{} more source lines omitted",
                    stats
                        .total_source_lines
                        .saturating_sub(stats.shown_source_lines)
                )
                .dim(),
            ]));
        }
    }

    out
}

fn render_change_header(rows: Vec<Row>, cwd: &Path) -> Vec<RtLine<'static>> {
    let total_added: usize = rows.iter().map(|r| r.added).sum();
    let total_removed: usize = rows.iter().map(|r| r.removed).sum();
    let file_count = rows.len();
    let noun = if file_count == 1 { "file" } else { "files" };
    let mut header_spans: Vec<RtSpan<'static>> = vec!["• ".dim()];
    if let [row] = &rows[..] {
        let verb = match &row.change {
            FileChange::Add { .. } => "Added",
            FileChange::Delete { .. } => "Deleted",
            _ => "Edited",
        };
        header_spans.push(verb.bold());
        header_spans.push(" ".into());
        header_spans.extend(render_row_path(row, cwd));
        header_spans.push(" ".into());
        header_spans.extend(render_line_count_summary(row.added, row.removed));
    } else {
        header_spans.push("Edited".bold());
        header_spans.push(format!(" {file_count} {noun} ").into());
        header_spans.extend(render_line_count_summary(total_added, total_removed));
    }

    vec![RtLine::from(header_spans)]
}

fn render_row_path(row: &Row, cwd: &Path) -> Vec<RtSpan<'static>> {
    let mut spans = Vec::new();
    spans.push(display_path_for(&row.path, cwd).into());
    if let Some(move_path) = &row.move_path {
        spans.push(format!(" → {}", display_path_for(move_path, cwd)).into());
    }
    spans
}

/// Detect the programming language for a file path by its extension.
/// Returns the raw extension string for `normalize_lang` / `find_syntax`
/// to resolve downstream.
#[cfg(test)]
fn detect_lang_for_path(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    Some(ext.to_string())
}
