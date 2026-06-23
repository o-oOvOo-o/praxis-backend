use super::*;

/// Returns the human-readable column header for the given sort key.
fn sort_key_label(sort_key: ThreadSortKey) -> &'static str {
    match sort_key {
        ThreadSortKey::CreatedAt => "Created at",
        ThreadSortKey::UpdatedAt => "Updated at",
    }
}

pub(super) fn draw_picker(tui: &mut Tui, state: &PickerState) -> std::io::Result<()> {
    // Render full-screen overlay
    let height = tui.terminal.size()?.height;
    tui.draw(height, |frame| {
        let area = frame.area();
        let [header, search, columns, list, hint] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(area.height.saturating_sub(4)),
            Constraint::Length(1),
        ])
        .areas(area);

        // Header
        let header_line = picker_header_line(state);
        frame.render_widget_ref(&header_line, header);

        // Search line
        let search_line = search_line(state);
        frame.render_widget_ref(&search_line, search);

        let (start, end) = visible_row_range(
            state.list_item_count(),
            state.scroll_top,
            list.height as usize,
        );
        let row_start = start.min(state.filtered_rows.len());
        let row_end = end.min(state.filtered_rows.len());
        let metrics = calculate_column_metrics_for_range(
            &state.filtered_rows,
            row_start,
            row_end,
            state.show_all,
        );

        // Column headers and list
        render_column_headers(frame, columns, &metrics, state.sort_key);
        render_list(frame, list, state, &metrics);

        // Hint line
        let hint_line = picker_hint_line(state);
        frame.render_widget_ref(&hint_line, hint);
    })
}

fn picker_header_line(state: &PickerState) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![state.effective_action().title().bold().cyan()];

    if state.shows_source_section() {
        spans.push("  ".into());
        spans.push("Source:".dim());
        spans.push(" ".into());
        if let Some(switcher) = state.source_switcher.as_ref() {
            for (index, source) in switcher.sources().enumerate() {
                if index > 0 {
                    spans.push(" ".into());
                }
                spans.push(source_tab_span(source, state.active_source));
            }
        } else {
            spans.push(source_tab_span(state.active_source, state.active_source));
        }
    }

    spans.push("  ".into());
    spans.push("Sort:".dim());
    spans.push(" ".into());
    spans.push(sort_key_label(state.sort_key).magenta());
    spans.into()
}

fn picker_hint_line(state: &PickerState) -> Line<'static> {
    let action_label = if matches!(state.action, SessionPickerAction::Resume)
        && state.active_source.is_external()
    {
        "fork into Praxis"
    } else {
        state.effective_action().action_label()
    };

    let mut spans: Vec<Span<'static>> = vec![
        key_hint::plain(KeyCode::Enter).into(),
        format!(" to {action_label} ").dim(),
        "    ".dim(),
        key_hint::plain(KeyCode::Esc).into(),
        " to start new ".dim(),
        "    ".dim(),
        key_hint::ctrl(KeyCode::Char('c')).into(),
        " to quit ".dim(),
    ];

    if state.has_source_switcher() {
        spans.push("    ".dim());
        spans.push(key_hint::plain(KeyCode::Left).into());
        spans.push("/".dim());
        spans.push(key_hint::plain(KeyCode::Right).into());
        spans.push(" to switch source ".dim());
    }

    spans.push("    ".dim());
    spans.push(key_hint::plain(KeyCode::Tab).into());
    spans.push(" to toggle sort ".dim());
    spans.push("    ".dim());
    spans.push(key_hint::plain(KeyCode::Up).into());
    spans.push("/".dim());
    spans.push(key_hint::plain(KeyCode::Down).into());
    spans.push(" to browse".dim());
    spans.into()
}

fn source_tab_span(
    source: SessionLookupSource,
    active_source: SessionLookupSource,
) -> Span<'static> {
    let label = source_display_name(source);
    if source == active_source {
        format!("[{label}]").bold().cyan()
    } else {
        label.dim()
    }
}

fn source_display_name(source: SessionLookupSource) -> &'static str {
    source.display_name()
}

fn search_line(state: &PickerState) -> Line<'_> {
    if let Some(error) = state.inline_error.as_deref() {
        return Line::from(error.red());
    }
    if state.query.is_empty() {
        return Line::from("Type to search".dim());
    }
    Line::from(format!("Search: {}", state.query))
}

fn visible_row_range(len: usize, scroll_top: usize, capacity: usize) -> (usize, usize) {
    if len == 0 || capacity == 0 {
        return (0, 0);
    }
    let start = scroll_top.min(len.saturating_sub(1));
    let end = len.min(start.saturating_add(capacity));
    (start, end)
}

fn render_list(
    frame: &mut crate::custom_terminal::Frame,
    area: Rect,
    state: &PickerState,
    metrics: &ColumnMetrics,
) {
    if area.height == 0 {
        return;
    }

    let rows = &state.filtered_rows;
    if state.list_item_count() == 0 {
        let message = render_empty_state_line(state);
        frame.render_widget_ref(&message, area);
        return;
    }

    let (start, end) = visible_row_range(
        state.list_item_count(),
        state.scroll_top,
        area.height as usize,
    );
    let row_start = start.min(rows.len());
    let row_end = end.min(rows.len());
    let labels = &metrics.labels;
    let label_start = row_start.saturating_sub(metrics.first_row);
    let label_end = label_start + row_end.saturating_sub(row_start);
    let mut y = area.y;

    let visibility = column_visibility(area.width, metrics, state.sort_key);
    let max_created_width = metrics.max_created_width;
    let max_updated_width = metrics.max_updated_width;
    let max_branch_width = metrics.max_branch_width;
    let max_cwd_width = metrics.max_cwd_width;

    for (idx, (row, (created_label, updated_label, branch_label, cwd_label))) in rows
        [row_start..row_end]
        .iter()
        .zip(labels[label_start..label_end].iter())
        .enumerate()
    {
        let is_sel = row_start + idx == state.selected;
        let marker = if is_sel { "> ".bold() } else { "  ".into() };
        let marker_width = 2usize;
        let created_span = if visibility.show_created {
            Some(Span::from(format!("{created_label:<max_created_width$}")).dim())
        } else {
            None
        };
        let updated_span = if visibility.show_updated {
            Some(Span::from(format!("{updated_label:<max_updated_width$}")).dim())
        } else {
            None
        };
        let branch_span = if !visibility.show_branch {
            None
        } else if branch_label.is_empty() {
            Some(
                Span::from(format!(
                    "{empty:<width$}",
                    empty = "-",
                    width = max_branch_width
                ))
                .dim(),
            )
        } else {
            Some(Span::from(format!("{branch_label:<max_branch_width$}")).cyan())
        };
        let cwd_span = if !visibility.show_cwd {
            None
        } else if cwd_label.is_empty() {
            Some(
                Span::from(format!(
                    "{empty:<width$}",
                    empty = "-",
                    width = max_cwd_width
                ))
                .dim(),
            )
        } else {
            Some(Span::from(format!("{cwd_label:<max_cwd_width$}")).dim())
        };

        let mut preview_width = area.width as usize;
        preview_width = preview_width.saturating_sub(marker_width);
        if visibility.show_created {
            preview_width = preview_width.saturating_sub(max_created_width + 2);
        }
        if visibility.show_updated {
            preview_width = preview_width.saturating_sub(max_updated_width + 2);
        }
        if visibility.show_branch {
            preview_width = preview_width.saturating_sub(max_branch_width + 2);
        }
        if visibility.show_cwd {
            preview_width = preview_width.saturating_sub(max_cwd_width + 2);
        }
        let add_leading_gap = !visibility.show_created
            && !visibility.show_updated
            && !visibility.show_branch
            && !visibility.show_cwd;
        if add_leading_gap {
            preview_width = preview_width.saturating_sub(2);
        }
        let preview = truncate_text(row.display_preview(), preview_width);
        let mut spans: Vec<Span> = vec![marker];
        if let Some(created) = created_span {
            spans.push(created);
            spans.push("  ".into());
        }
        if let Some(updated) = updated_span {
            spans.push(updated);
            spans.push("  ".into());
        }
        if let Some(branch) = branch_span {
            spans.push(branch);
            spans.push("  ".into());
        }
        if let Some(cwd) = cwd_span {
            spans.push(cwd);
            spans.push("  ".into());
        }
        if add_leading_gap {
            spans.push("  ".into());
        }
        spans.push(preview.into());

        let line: Line = spans.into();
        let rect = Rect::new(area.x, y, area.width, 1);
        frame.render_widget_ref(&line, rect);
        y = y.saturating_add(1);
    }

    let rendered_load_more = state.has_load_more_row()
        && start <= rows.len()
        && rows.len() < end
        && y < area.y.saturating_add(area.height);
    if rendered_load_more {
        let selected = state.is_load_more_index(state.selected);
        let line = render_load_more_line(selected, state.pagination.loading.is_pending());
        let rect = Rect::new(area.x, y, area.width, 1);
        frame.render_widget_ref(&line, rect);
        y = y.saturating_add(1);
    }

    if state.pagination.loading.is_pending()
        && !rendered_load_more
        && y < area.y.saturating_add(area.height)
    {
        let loading_line: Line = vec!["  ".into(), "Loading older sessions…".italic().dim()].into();
        let rect = Rect::new(area.x, y, area.width, 1);
        frame.render_widget_ref(&loading_line, rect);
    }
}

fn render_load_more_line(selected: bool, loading: bool) -> Line<'static> {
    let marker = if selected { "> ".bold() } else { "  ".into() };
    let label = if loading {
        "Loading older sessions…".italic().dim()
    } else {
        "Load more".cyan()
    };
    vec![marker, label].into()
}

fn render_empty_state_line(state: &PickerState) -> Line<'static> {
    if !state.query.is_empty() {
        if state.search_state.is_active()
            || (state.pagination.loading.is_pending() && state.pagination.cursors.has_next_page())
        {
            return vec!["Searching…".italic().dim()].into();
        }
        if state.pagination.reached_scan_cap {
            let msg = format!(
                "Search scanned first {} sessions; more may exist",
                state.pagination.num_scanned_files
            );
            return vec![Span::from(msg).italic().dim()].into();
        }
        return vec!["No results for your search".italic().dim()].into();
    }

    if state.all_rows.is_empty() && state.pagination.num_scanned_files == 0 {
        let message = if state.shows_source_section() {
            format!(
                "No {} sessions yet",
                source_display_name(state.active_source)
            )
        } else {
            String::from("No sessions yet")
        };
        return vec![Span::from(message).italic().dim()].into();
    }

    if state.pagination.loading.is_pending() {
        return vec!["Loading older sessions…".italic().dim()].into();
    }

    vec!["No sessions yet".italic().dim()].into()
}

fn human_time_ago(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now - ts;
    let secs = delta.num_seconds();
    if secs < 60 {
        let n = secs.max(0);
        if n == 1 {
            format!("{n} second ago")
        } else {
            format!("{n} seconds ago")
        }
    } else if secs < 60 * 60 {
        let m = secs / 60;
        if m == 1 {
            format!("{m} minute ago")
        } else {
            format!("{m} minutes ago")
        }
    } else if secs < 60 * 60 * 24 {
        let h = secs / 3600;
        if h == 1 {
            format!("{h} hour ago")
        } else {
            format!("{h} hours ago")
        }
    } else {
        let d = secs / (60 * 60 * 24);
        if d == 1 {
            format!("{d} day ago")
        } else {
            format!("{d} days ago")
        }
    }
}

fn format_updated_label(row: &Row) -> String {
    match (row.updated_at, row.created_at) {
        (Some(updated), _) => human_time_ago(updated),
        (None, Some(created)) => human_time_ago(created),
        (None, None) => "-".to_string(),
    }
}

fn format_created_label(row: &Row) -> String {
    match row.created_at {
        Some(created) => human_time_ago(created),
        None => "-".to_string(),
    }
}

fn render_column_headers(
    frame: &mut crate::custom_terminal::Frame,
    area: Rect,
    metrics: &ColumnMetrics,
    sort_key: ThreadSortKey,
) {
    if area.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = vec!["  ".into()];
    let visibility = column_visibility(area.width, metrics, sort_key);
    if visibility.show_created {
        let label = format!(
            "{text:<width$}",
            text = "Created at",
            width = metrics.max_created_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    if visibility.show_updated {
        let label = format!(
            "{text:<width$}",
            text = "Updated at",
            width = metrics.max_updated_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    if visibility.show_branch {
        let label = format!(
            "{text:<width$}",
            text = "Branch",
            width = metrics.max_branch_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    if visibility.show_cwd {
        let label = format!(
            "{text:<width$}",
            text = "CWD",
            width = metrics.max_cwd_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    spans.push("Conversation".bold());
    let line = Line::from(spans);
    frame.render_widget_ref(&line, area);
}

/// Pre-computed column widths and formatted labels for all visible rows.
///
/// Widths are measured in Unicode display width (not byte length) so columns
/// align correctly when labels contain non-ASCII characters.
struct ColumnMetrics {
    first_row: usize,
    max_created_width: usize,
    max_updated_width: usize,
    max_branch_width: usize,
    max_cwd_width: usize,
    /// (created_label, updated_label, branch_label, cwd_label) per row.
    labels: Vec<(String, String, String, String)>,
}

/// Determines which columns to render given available terminal width.
///
/// When the terminal is narrow, only one timestamp column is shown (whichever
/// matches the current sort key). Branch and CWD are hidden if their max
/// widths are zero (no data to show).
#[derive(Debug, PartialEq, Eq)]
struct ColumnVisibility {
    show_created: bool,
    show_updated: bool,
    show_branch: bool,
    show_cwd: bool,
}

#[cfg(test)]
fn calculate_column_metrics(rows: &[Row], include_cwd: bool) -> ColumnMetrics {
    calculate_column_metrics_for_range(rows, 0, rows.len(), include_cwd)
}

fn calculate_column_metrics_for_range(
    rows: &[Row],
    first_row: usize,
    last_row: usize,
    include_cwd: bool,
) -> ColumnMetrics {
    fn right_elide(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            return s.to_string();
        }
        if max <= 1 {
            return "…".to_string();
        }
        let tail_len = max - 1;
        let tail: String = s
            .chars()
            .rev()
            .take(tail_len)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("…{tail}")
    }

    let mut labels: Vec<(String, String, String, String)> =
        Vec::with_capacity(last_row.saturating_sub(first_row));
    let mut max_created_width = UnicodeWidthStr::width("Created at");
    let mut max_updated_width = UnicodeWidthStr::width("Updated at");
    let mut max_branch_width = UnicodeWidthStr::width("Branch");
    let mut max_cwd_width = if include_cwd {
        UnicodeWidthStr::width("CWD")
    } else {
        0
    };

    for row in rows.get(first_row..last_row).unwrap_or(&[]) {
        let created = format_created_label(row);
        let updated = format_updated_label(row);
        let branch_raw = row.git_branch.clone().unwrap_or_default();
        let branch = right_elide(&branch_raw, /*max*/ 24);
        let cwd = if include_cwd {
            let cwd_raw = row
                .cwd
                .as_ref()
                .map(|p| display_path_for(p, std::path::Path::new("/")))
                .unwrap_or_default();
            right_elide(&cwd_raw, /*max*/ 24)
        } else {
            String::new()
        };
        max_created_width = max_created_width.max(UnicodeWidthStr::width(created.as_str()));
        max_updated_width = max_updated_width.max(UnicodeWidthStr::width(updated.as_str()));
        max_branch_width = max_branch_width.max(UnicodeWidthStr::width(branch.as_str()));
        max_cwd_width = max_cwd_width.max(UnicodeWidthStr::width(cwd.as_str()));
        labels.push((created, updated, branch, cwd));
    }

    ColumnMetrics {
        first_row,
        max_created_width,
        max_updated_width,
        max_branch_width,
        max_cwd_width,
        labels,
    }
}

/// Computes which columns fit in the available width.
///
/// The algorithm reserves at least `MIN_PREVIEW_WIDTH` characters for the
/// conversation preview. If both timestamp columns don't fit, only the one
/// matching the current sort key is shown.
fn column_visibility(
    area_width: u16,
    metrics: &ColumnMetrics,
    sort_key: ThreadSortKey,
) -> ColumnVisibility {
    const MIN_PREVIEW_WIDTH: usize = 10;

    let show_branch = metrics.max_branch_width > 0;
    let show_cwd = metrics.max_cwd_width > 0;

    // Calculate remaining width after all optional columns.
    let mut preview_width = area_width as usize;
    preview_width = preview_width.saturating_sub(2); // marker
    if metrics.max_created_width > 0 {
        preview_width = preview_width.saturating_sub(metrics.max_created_width + 2);
    }
    if metrics.max_updated_width > 0 {
        preview_width = preview_width.saturating_sub(metrics.max_updated_width + 2);
    }
    if show_branch {
        preview_width = preview_width.saturating_sub(metrics.max_branch_width + 2);
    }
    if show_cwd {
        preview_width = preview_width.saturating_sub(metrics.max_cwd_width + 2);
    }

    // If preview would be too narrow, hide the non-active timestamp column.
    let show_both = preview_width >= MIN_PREVIEW_WIDTH;
    let show_created = if show_both {
        metrics.max_created_width > 0
    } else {
        sort_key == ThreadSortKey::CreatedAt
    };
    let show_updated = if show_both {
        metrics.max_updated_width > 0
    } else {
        sort_key == ThreadSortKey::UpdatedAt
    };

    ColumnVisibility {
        show_created,
        show_updated,
        show_branch,
        show_cwd,
    }
}
