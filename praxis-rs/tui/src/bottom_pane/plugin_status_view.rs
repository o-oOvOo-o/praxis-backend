use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;
use serde::Deserialize;

use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::ColumnWidthMode;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::measure_rows_height_with_col_width_mode;
use super::selection_popup_common::render_menu_surface;
use super::selection_popup_common::render_rows_with_col_width_mode;
use crate::key_hint;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;
use crate::surface;

pub(crate) const PLUGIN_STATUS_VIEW_ID: &str = "plugin-status-view";

const GRID_COLUMNS: usize = 3;
const MIN_CARD_WIDTH: u16 = 24;
const CARD_HEIGHT: u16 = 7;
const CARD_GAP_X: u16 = 1;
const CARD_GAP_Y: u16 = 1;
const OVERVIEW_MAX_VISIBLE_CARD_ROWS: usize = 4;
const DETAIL_MAX_VISIBLE_ROWS: usize = 16;
const WHEEL_DETAIL_LINES: usize = 3;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginStatusDocument {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) subtitle: Option<String>,
    #[serde(default)]
    pub(crate) filters: Vec<PluginStatusFilter>,
    #[serde(default)]
    pub(crate) rows: Vec<PluginStatusRow>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginStatusFilter {
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) options: Vec<PluginStatusFilterOption>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginStatusFilterOption {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginStatusRow {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) category: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) progress_percent: Option<f64>,
    #[serde(default)]
    pub(crate) filter: Option<String>,
    #[serde(default)]
    pub(crate) details: Vec<String>,
}

pub(crate) struct PluginStatusView {
    document: PluginStatusDocument,
    state: ScrollState,
    filter_index: usize,
    dropdown_open: bool,
    dropdown_state: ScrollState,
    detail_open: bool,
    detail_scroll: usize,
    complete: bool,
}

impl PluginStatusDocument {
    pub(crate) fn loading(title: String, subtitle: String) -> Self {
        Self {
            title,
            subtitle: Some(subtitle),
            filters: Vec::new(),
            rows: vec![PluginStatusRow {
                name: "Loading".to_string(),
                description: Some("Waiting for plugin command output".to_string()),
                category: Some("Status".to_string()),
                status: None,
                progress_percent: None,
                filter: None,
                details: Vec::new(),
            }],
        }
    }

    pub(crate) fn error(title: String, message: String) -> Self {
        Self {
            title,
            subtitle: Some("Plugin command failed".to_string()),
            filters: Vec::new(),
            rows: vec![PluginStatusRow {
                name: "Error".to_string(),
                description: Some(message),
                category: Some("Status".to_string()),
                status: None,
                progress_percent: None,
                filter: None,
                details: Vec::new(),
            }],
        }
    }

    pub(crate) fn process_output(
        title: String,
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
    ) -> Self {
        let mut details = if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
            serde_json::to_string_pretty(&value)
                .unwrap_or(stdout)
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
        } else {
            stdout.lines().map(str::to_string).collect::<Vec<_>>()
        };
        if !stderr.is_empty() {
            details.push("stderr".to_string());
            details.extend(stderr.lines().map(str::to_string));
        }
        let succeeded = exit_code == Some(0);
        Self {
            title,
            subtitle: Some(format!(
                "Process exited with {}",
                exit_code.map_or_else(
                    || "unknown status".to_string(),
                    |code| format!("code {code}")
                )
            )),
            filters: Vec::new(),
            rows: vec![PluginStatusRow {
                name: if succeeded { "Completed" } else { "Failed" }.to_string(),
                description: Some("Canonical plugin process output".to_string()),
                category: Some("Command".to_string()),
                status: Some(if succeeded { "Ready" } else { "Error" }.to_string()),
                progress_percent: Some(if succeeded { 100.0 } else { 0.0 }),
                filter: None,
                details,
            }],
        }
    }
}

impl PluginStatusView {
    pub(crate) fn new(document: PluginStatusDocument) -> Self {
        let state = ScrollState::new();
        let mut dropdown_state = ScrollState::new();
        let mut view = Self {
            document,
            state,
            filter_index: 0,
            dropdown_open: false,
            dropdown_state,
            detail_open: false,
            detail_scroll: 0,
            complete: false,
        };
        view.state.clamp_selection(view.filtered_rows().len());
        dropdown_state.clamp_selection(view.filter_options().len());
        view.dropdown_state = dropdown_state;
        view
    }

    fn active_filter(&self) -> Option<&PluginStatusFilterOption> {
        self.filter_options().get(self.filter_index)
    }

    fn filter_options(&self) -> &[PluginStatusFilterOption] {
        self.document
            .filters
            .first()
            .map(|filter| filter.options.as_slice())
            .unwrap_or_default()
    }

    fn filter_label(&self) -> &str {
        self.document
            .filters
            .first()
            .map(|filter| filter.label.as_str())
            .unwrap_or("Filter")
    }

    fn row_matches_active_filter(&self, row: &PluginStatusRow) -> bool {
        match self.active_filter().map(|filter| filter.id.as_str()) {
            None | Some("all") => true,
            Some(selected) => row.filter.as_deref() == Some(selected),
        }
    }

    fn filtered_rows(&self) -> Vec<&PluginStatusRow> {
        self.document
            .rows
            .iter()
            .filter(|row| self.row_matches_active_filter(row))
            .collect()
    }

    fn selected_row(&self) -> Option<&PluginStatusRow> {
        let selected = self.state.selected_idx?;
        self.document
            .rows
            .iter()
            .filter(|row| self.row_matches_active_filter(row))
            .nth(selected)
    }

    fn dropdown_rows(&self) -> Vec<GenericDisplayRow> {
        self.filter_options()
            .iter()
            .enumerate()
            .map(|(index, option)| GenericDisplayRow {
                name: option.label.clone(),
                name_prefix_spans: Vec::new(),
                display_shortcut: None,
                match_indices: None,
                description: (index == self.filter_index).then_some("current".to_string()),
                category_tag: Some(self.filter_label().to_string()),
                disabled_reason: None,
                is_disabled: false,
                wrap_indent: None,
            })
            .collect()
    }

    fn overview_columns(width: u16) -> usize {
        if width == 0 {
            return 1;
        }
        let fit = ((width.saturating_add(CARD_GAP_X)) / MIN_CARD_WIDTH.saturating_add(CARD_GAP_X))
            .max(1) as usize;
        fit.min(GRID_COLUMNS)
    }

    fn visible_card_rows_for_height(height: u16, total_rows: usize) -> usize {
        if total_rows == 0 {
            return 0;
        }
        let extent = CARD_HEIGHT.saturating_add(CARD_GAP_Y).max(1);
        let visible = height
            .saturating_add(CARD_GAP_Y)
            .checked_div(extent)
            .unwrap_or(1)
            .max(1) as usize;
        visible.min(OVERVIEW_MAX_VISIBLE_CARD_ROWS).min(total_rows)
    }

    fn visible_grid_height(visible_rows: usize) -> u16 {
        if visible_rows == 0 {
            return 1;
        }
        let gaps = visible_rows.saturating_sub(1) as u16;
        (visible_rows as u16)
            .saturating_mul(CARD_HEIGHT)
            .saturating_add(gaps.saturating_mul(CARD_GAP_Y))
    }

    fn overview_grid_height_for_rows(row_count: usize, width: u16) -> u16 {
        if row_count == 0 {
            return 1;
        }
        let columns = Self::overview_columns(width);
        let total_card_rows = (row_count + columns - 1) / columns;
        Self::visible_grid_height(total_card_rows.min(OVERVIEW_MAX_VISIBLE_CARD_ROWS))
    }

    fn ensure_overview_selection_visible(&mut self) {
        let len = self.filtered_rows().len();
        self.state.clamp_selection(len);
        self.state
            .ensure_visible(len, OVERVIEW_MAX_VISIBLE_CARD_ROWS * GRID_COLUMNS);
    }

    fn move_overview_left(&mut self) {
        let len = self.filtered_rows().len();
        if len == 0 {
            self.state.clamp_selection(0);
            return;
        }
        let current = self.state.selected_idx.unwrap_or(0).min(len - 1);
        self.state.selected_idx = Some(if current == 0 { len - 1 } else { current - 1 });
        self.ensure_overview_selection_visible();
    }

    fn move_overview_right(&mut self) {
        let len = self.filtered_rows().len();
        if len == 0 {
            self.state.clamp_selection(0);
            return;
        }
        let current = self.state.selected_idx.unwrap_or(0).min(len - 1);
        self.state.selected_idx = Some((current + 1) % len);
        self.ensure_overview_selection_visible();
    }

    fn move_overview_up(&mut self) {
        let len = self.filtered_rows().len();
        if len == 0 {
            self.state.clamp_selection(0);
            return;
        }
        let current = self.state.selected_idx.unwrap_or(0).min(len - 1);
        let next = if current >= GRID_COLUMNS {
            current - GRID_COLUMNS
        } else {
            let col = current % GRID_COLUMNS;
            let last_row = (len - 1) / GRID_COLUMNS;
            let mut candidate = last_row * GRID_COLUMNS + col;
            while candidate >= len {
                candidate = candidate.saturating_sub(GRID_COLUMNS);
            }
            candidate
        };
        self.state.selected_idx = Some(next);
        self.ensure_overview_selection_visible();
    }

    fn move_overview_down(&mut self) {
        let len = self.filtered_rows().len();
        if len == 0 {
            self.state.clamp_selection(0);
            return;
        }
        let current = self.state.selected_idx.unwrap_or(0).min(len - 1);
        let next = if current + GRID_COLUMNS < len {
            current + GRID_COLUMNS
        } else {
            (current % GRID_COLUMNS).min(len - 1)
        };
        self.state.selected_idx = Some(next);
        self.ensure_overview_selection_visible();
    }

    fn move_up(&mut self) {
        if self.dropdown_open {
            let len = self.filter_options().len();
            self.dropdown_state.move_up_wrap(len);
            self.dropdown_state
                .ensure_visible(len, MAX_POPUP_ROWS.min(len));
            return;
        }
        if self.detail_open {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
            return;
        }
        self.move_overview_up();
    }

    fn move_down(&mut self) {
        if self.dropdown_open {
            let len = self.filter_options().len();
            self.dropdown_state.move_down_wrap(len);
            self.dropdown_state
                .ensure_visible(len, MAX_POPUP_ROWS.min(len));
            return;
        }
        if self.detail_open {
            self.scroll_detail(1);
            return;
        }
        self.move_overview_down();
    }

    fn move_left(&mut self) {
        if !self.dropdown_open && !self.detail_open {
            self.move_overview_left();
        }
    }

    fn move_right(&mut self) {
        if !self.dropdown_open && !self.detail_open {
            self.move_overview_right();
        }
    }

    fn scroll_dropdown(&mut self, delta_rows: isize) {
        let len = self.filter_options().len();
        if len == 0 {
            self.dropdown_state.clamp_selection(0);
            return;
        }
        let current = self.dropdown_state.selected_idx.unwrap_or(0).min(len - 1);
        let next = if delta_rows.is_negative() {
            current.saturating_sub(delta_rows.unsigned_abs())
        } else {
            current
                .saturating_add(delta_rows as usize)
                .min(len.saturating_sub(1))
        };
        self.dropdown_state.selected_idx = Some(next);
        self.dropdown_state
            .ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    fn scroll_detail(&mut self, delta_lines: isize) {
        if delta_lines.is_negative() {
            self.detail_scroll = self
                .detail_scroll
                .saturating_sub(delta_lines.unsigned_abs());
            return;
        }
        let max_scroll = self
            .selected_row()
            .map(|row| self.detail_lines(row, 80).len().saturating_sub(1))
            .unwrap_or(0);
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(delta_lines as usize)
            .min(max_scroll);
    }

    fn scroll_overview_rows(&mut self, delta_rows: isize) {
        let len = self.filtered_rows().len();
        if len == 0 {
            self.state.clamp_selection(0);
            return;
        }
        let current = self.state.selected_idx.unwrap_or(0).min(len - 1);
        let step = GRID_COLUMNS.saturating_mul(delta_rows.unsigned_abs());
        let next = if delta_rows.is_negative() {
            current.saturating_sub(step)
        } else {
            current.saturating_add(step).min(len.saturating_sub(1))
        };
        self.state.selected_idx = Some(next);
        self.ensure_overview_selection_visible();
    }

    fn scroll_view(&mut self, delta_rows: isize) {
        if self.dropdown_open {
            self.scroll_dropdown(delta_rows);
        } else if self.detail_open {
            self.scroll_detail(delta_rows.saturating_mul(WHEEL_DETAIL_LINES as isize));
        } else {
            self.scroll_overview_rows(delta_rows);
        }
    }

    fn toggle_dropdown(&mut self) {
        if self.detail_open {
            return;
        }
        self.dropdown_open = !self.dropdown_open;
        if self.dropdown_open {
            self.dropdown_state.selected_idx = Some(self.filter_index);
            self.dropdown_state
                .ensure_visible(self.filter_options().len(), MAX_POPUP_ROWS);
        }
    }

    fn accept_dropdown(&mut self) {
        if !self.dropdown_open {
            return;
        }
        if let Some(index) = self.dropdown_state.selected_idx {
            self.filter_index = index.min(self.filter_options().len().saturating_sub(1));
            self.state.clamp_selection(self.filtered_rows().len());
            self.ensure_overview_selection_visible();
        }
        self.dropdown_open = false;
    }

    fn open_detail(&mut self) {
        if self.dropdown_open {
            self.accept_dropdown();
            return;
        }
        if self.selected_row().is_some() {
            self.detail_open = true;
            self.detail_scroll = 0;
        }
    }

    fn close_or_back(&mut self) {
        if self.dropdown_open {
            self.dropdown_open = false;
        } else if self.detail_open {
            self.detail_open = false;
            self.detail_scroll = 0;
        } else {
            self.complete = true;
        }
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        let selected_filter = self
            .active_filter()
            .map(|filter| filter.label.as_str())
            .unwrap_or("All");
        let chevron = if self.dropdown_open { "^" } else { "v" };
        let mut title_line = vec![self.document.title.clone().bold()];
        if let Some(subtitle) = self.document.subtitle.as_deref() {
            title_line.push("  ".into());
            title_line.push(subtitle.to_string().dim());
        }
        vec![
            title_line.into(),
            vec![
                Span::from(self.filter_label().to_string()).dim(),
                ": ".dim(),
                selected_filter.to_string().cyan(),
                format!(" {chevron}").dim(),
            ]
            .into(),
        ]
    }

    fn detail_header_lines(&self, row: &PluginStatusRow) -> Vec<Line<'static>> {
        vec![
            row.name.clone().bold().into(),
            vec![
                "Status: ".dim(),
                Span::styled(Self::status_label(row).to_string(), Self::status_style(row)),
            ]
            .into(),
        ]
    }

    fn status_label(row: &PluginStatusRow) -> &str {
        row.status
            .as_deref()
            .or(row.filter.as_deref())
            .filter(|status| !status.trim().is_empty())
            .unwrap_or("Unknown")
    }

    fn status_style(row: &PluginStatusRow) -> Style {
        match Self::status_label(row).to_ascii_lowercase().as_str() {
            "ready" | "done" | "complete" | "completed" => Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            "active" | "running" | "in_progress" | "in-progress" => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            "blocked" | "error" | "failed" => {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            }
            "open" | "todo" | "pending" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(surface::runtime_theme().muted),
        }
    }

    fn normalized_progress(progress: Option<f64>) -> f64 {
        progress.unwrap_or(0.0).clamp(0.0, 100.0)
    }

    fn progress_line(
        progress: Option<f64>,
        max_width: usize,
        include_label: bool,
    ) -> Line<'static> {
        let theme = surface::runtime_theme();
        let progress = Self::normalized_progress(progress);
        let percent = format!("{progress:>3.0}%");
        let prefix = if include_label { "Progress: " } else { "" };
        let reserved = prefix.len().saturating_add(percent.len()).saturating_add(3);
        let bar_width = max_width.saturating_sub(reserved).clamp(4, 32);
        let filled = ((bar_width as f64 * progress) / 100.0).round() as usize;
        let filled = filled.min(bar_width);
        let empty = bar_width.saturating_sub(filled);
        let mut spans = Vec::new();
        if include_label {
            spans.push(prefix.dim());
        }
        spans.push("[".dim());
        spans.push(Span::styled(
            "=".repeat(filled),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "-".repeat(empty),
            Style::default().fg(theme.dim),
        ));
        spans.push("] ".dim());
        spans.push(Span::styled(
            percent,
            Style::default().fg(theme.text_strong),
        ));
        truncate_line_with_ellipsis_if_overflow(Line::from(spans), max_width)
    }

    fn detail_lines(&self, row: &PluginStatusRow, width: u16) -> Vec<Line<'static>> {
        let width = width.max(1) as usize;
        let mut lines = vec![
            Self::progress_line(row.progress_percent, width, true),
            Line::from(""),
        ];
        if let Some(description) = row.description.as_deref().filter(|value| !value.is_empty()) {
            lines.push("Summary".bold().into());
            lines.push(description.to_string().dim().into());
            lines.push(Line::from(""));
        }
        if let Some(category) = row.category.as_deref().filter(|value| !value.is_empty()) {
            lines.push(vec!["Category: ".dim(), category.to_string().into()].into());
        }
        if !row.details.is_empty() {
            lines.push("Details".bold().into());
            for detail in &row.details {
                lines.push(vec!["- ".dim(), detail.clone().into()].into());
            }
        } else if row.description.is_none() {
            lines.push("No detail rows provided.".dim().into());
        }
        lines
    }

    fn footer_line(&self) -> Line<'static> {
        let spans = if self.detail_open {
            vec![
                Span::from(key_hint::plain(KeyCode::Up)).dim(),
                "/".dim(),
                Span::from(key_hint::plain(KeyCode::Down)).dim(),
                " scroll  ".dim(),
                "wheel scroll  ".dim(),
                Span::from(key_hint::plain(KeyCode::Esc)).dim(),
                " back".dim(),
            ]
        } else {
            vec![
                Span::from(key_hint::plain(KeyCode::Up)).dim(),
                "/".dim(),
                Span::from(key_hint::plain(KeyCode::Down)).dim(),
                " move  ".dim(),
                Span::from(key_hint::plain(KeyCode::Tab)).dim(),
                " filter  ".dim(),
                Span::from(key_hint::plain(KeyCode::Enter)).dim(),
                " details  ".dim(),
                "wheel scroll  ".dim(),
                Span::from(key_hint::plain(KeyCode::Esc)).dim(),
                " exit".dim(),
            ]
        };
        Line::from(spans)
    }

    fn render_line(area: Rect, buf: &mut Buffer, line: Line<'static>) {
        if area.is_empty() {
            return;
        }
        truncate_line_with_ellipsis_if_overflow(line, area.width as usize).render(area, buf);
    }

    fn card_inner(area: Rect) -> Rect {
        if area.width <= 2 || area.height <= 2 {
            return Rect {
                x: area.x,
                y: area.y,
                width: 0,
                height: 0,
            };
        }
        Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        }
    }

    fn row_area(area: Rect, offset: u16) -> Rect {
        Rect {
            x: area.x,
            y: area.y.saturating_add(offset),
            width: area.width,
            height: u16::from(offset < area.height),
        }
    }

    fn render_card(area: Rect, buf: &mut Buffer, row: &PluginStatusRow, selected: bool) {
        if area.width < 6 || area.height < 3 {
            return;
        }
        let theme = surface::runtime_theme();
        let base_style = if selected {
            Style::default().bg(theme.selected_bg).fg(theme.text_strong)
        } else {
            Style::default().bg(theme.panel_bg).fg(theme.text)
        };
        let border_style = if selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_muted)
        };
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(base_style)
            .render(area, buf);

        let inner = Self::card_inner(area);
        if inner.is_empty() {
            return;
        }
        let name = Line::from(row.name.clone()).bold();
        let mut status_spans = vec![Span::styled(
            Self::status_label(row).to_string(),
            Self::status_style(row),
        )];
        if let Some(category) = row.category.as_deref().filter(|value| !value.is_empty()) {
            status_spans.push("  ".into());
            status_spans.push(Span::from(category.to_string()).dim());
        }
        Self::render_line(Self::row_area(inner, 0), buf, name);
        Self::render_line(Self::row_area(inner, 1), buf, Line::from(status_spans));
        Self::render_line(
            Self::row_area(inner, 2),
            buf,
            Self::progress_line(row.progress_percent, inner.width as usize, false),
        );
        if let Some(description) = row.description.as_deref().filter(|value| !value.is_empty()) {
            Self::render_line(
                Self::row_area(inner, 3),
                buf,
                Line::from(Span::from(description.to_string()).dim()),
            );
        }
        if !row.details.is_empty() {
            Self::render_line(
                Self::row_area(inner, 4),
                buf,
                Line::from(Span::from(format!("{} detail rows", row.details.len())).dim()),
            );
        }
    }

    fn render_overview_grid(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.filtered_rows();
        if rows.is_empty() {
            Paragraph::new("no rows".dim()).render(area, buf);
            return;
        }

        let columns = Self::overview_columns(area.width);
        let total_card_rows = (rows.len() + columns - 1) / columns;
        let visible_card_rows = Self::visible_card_rows_for_height(area.height, total_card_rows);
        if visible_card_rows == 0 {
            return;
        }

        let selected = self.state.selected_idx.unwrap_or(0).min(rows.len() - 1);
        let selected_row = selected / columns;
        let mut top_row = (self.state.scroll_top / columns).min(total_card_rows.saturating_sub(1));
        if selected_row < top_row {
            top_row = selected_row;
        } else if selected_row >= top_row.saturating_add(visible_card_rows) {
            top_row = selected_row
                .saturating_add(1)
                .saturating_sub(visible_card_rows);
        }

        let total_gap_width = (columns.saturating_sub(1) as u16).saturating_mul(CARD_GAP_X);
        let available_width = area.width.saturating_sub(total_gap_width);
        let base_width = (available_width / columns as u16).max(1);
        let extra_width = available_width % columns as u16;
        let bottom = area.y.saturating_add(area.height);
        for visual_row in 0..visible_card_rows {
            let y = area.y.saturating_add(
                (visual_row as u16).saturating_mul(CARD_HEIGHT.saturating_add(CARD_GAP_Y)),
            );
            if y >= bottom {
                break;
            }
            let mut x = area.x;
            for col in 0..columns {
                let width = base_width + u16::from((col as u16) < extra_width);
                let index = (top_row + visual_row) * columns + col;
                if index >= rows.len() {
                    break;
                }
                let height = CARD_HEIGHT.min(bottom.saturating_sub(y));
                let card_area = Rect {
                    x,
                    y,
                    width,
                    height,
                };
                Self::render_card(card_area, buf, rows[index], index == selected);
                x = x.saturating_add(width).saturating_add(CARD_GAP_X);
            }
        }
    }

    fn render_detail(&self, area: Rect, buf: &mut Buffer) {
        let Some(row) = self.selected_row() else {
            Paragraph::new("no selected row".dim()).render(area, buf);
            return;
        };
        let lines = self.detail_lines(row, area.width);
        let max_scroll = lines.len().saturating_sub(area.height as usize);
        let scroll = self.detail_scroll.min(max_scroll) as u16;
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn render_dropdown(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.dropdown_rows();
        render_rows_with_col_width_mode(
            area,
            buf,
            &rows,
            &self.dropdown_state,
            MAX_POPUP_ROWS,
            "no rows",
            ColumnWidthMode::Fixed,
        );
    }
}

impl BottomPaneView for PluginStatusView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => self.close_or_back(),
            KeyEvent {
                code: KeyCode::Tab, ..
            }
            | KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.toggle_dropdown(),
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => self.open_detail(),
            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Left,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_left(),
            KeyEvent {
                code: KeyCode::Right,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.move_right(),
            _ => {}
        }
    }

    fn handle_mouse_event(&mut self, mouse_event: &MouseEvent) -> bool {
        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_view(-1);
                true
            }
            MouseEventKind::ScrollDown => {
                self.scroll_view(1);
                true
            }
            _ => false,
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn view_id(&self) -> Option<&'static str> {
        Some(PLUGIN_STATUS_VIEW_ID)
    }

    fn selected_index(&self) -> Option<usize> {
        self.state.selected_idx
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }
}

impl Renderable for PluginStatusView {
    fn desired_height(&self, width: u16) -> u16 {
        let header_height = if self.detail_open {
            self.selected_row()
                .map(|row| self.detail_header_lines(row).len() as u16)
                .unwrap_or(1)
        } else {
            self.header_lines().len() as u16
        };

        let content_height = if self.dropdown_open {
            let rows = self.dropdown_rows();
            let mut state = self.dropdown_state;
            let row_count = rows.len();
            state.ensure_visible(row_count, MAX_POPUP_ROWS.min(row_count));
            measure_rows_height_with_col_width_mode(
                &rows,
                &state,
                MAX_POPUP_ROWS,
                width.saturating_sub(4),
                ColumnWidthMode::Fixed,
            )
        } else if self.detail_open {
            self.selected_row()
                .map(|row| {
                    self.detail_lines(row, width.saturating_sub(4))
                        .len()
                        .min(DETAIL_MAX_VISIBLE_ROWS)
                        .max(1) as u16
                })
                .unwrap_or(1)
        } else {
            let row_count = self.filtered_rows().len();
            Self::overview_grid_height_for_rows(row_count, width.saturating_sub(4))
        };

        header_height
            .saturating_add(content_height)
            .saturating_add(3)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let inner = render_menu_surface(area, buf);
        if inner.is_empty() {
            return;
        }

        let header_lines = if self.detail_open {
            self.selected_row()
                .map(|row| self.detail_header_lines(row))
                .unwrap_or_else(|| vec![self.document.title.clone().bold().into()])
        } else {
            self.header_lines()
        };
        let layout = Layout::vertical([
            Constraint::Length(header_lines.len() as u16),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

        Paragraph::new(header_lines).render(layout[0], buf);
        if self.dropdown_open {
            self.render_dropdown(layout[1], buf);
        } else if self.detail_open {
            self.render_detail(layout[1], buf);
        } else {
            self.render_overview_grid(layout[1], buf);
        }
        Paragraph::new(self.footer_line()).render(layout[2], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::renderable::Renderable;

    fn status_document(row_count: usize) -> PluginStatusDocument {
        PluginStatusDocument {
            title: "Gaea Flywheel".to_string(),
            subtitle: Some(format!("Nodes {row_count}")),
            filters: Vec::new(),
            rows: (0..row_count)
                .map(|index| PluginStatusRow {
                    name: format!("Node {index}"),
                    description: Some("Flywheel graph contract evidence".to_string()),
                    category: Some("Graph".to_string()),
                    status: Some("Open".to_string()),
                    progress_percent: Some(20.0),
                    filter: Some("open".to_string()),
                    details: vec!["one".to_string(), "two".to_string()],
                })
                .collect(),
        }
    }

    #[test]
    fn overview_large_documents_stay_compact() {
        let view = PluginStatusView::new(status_document(43));
        let expected_grid_height =
            PluginStatusView::visible_grid_height(OVERVIEW_MAX_VISIBLE_CARD_ROWS);
        assert_eq!(
            view.desired_height(96),
            2 + expected_grid_height + 3,
            "large status documents should page instead of filling the terminal"
        );
    }

    #[test]
    fn overview_empty_documents_keep_a_small_body() {
        let view = PluginStatusView::new(status_document(0));
        assert_eq!(view.desired_height(96), 2 + 1 + 3);
    }

    #[test]
    fn process_output_preserves_non_panel_json_and_failure_status() {
        let document = PluginStatusDocument::process_output(
            "/gaea".to_string(),
            r#"{"failed_count":1,"artifact_dir":"D:/artifacts"}"#.to_string(),
            "strict gate failed".to_string(),
            Some(1),
        );

        assert_eq!(document.rows[0].name, "Failed");
        assert_eq!(document.rows[0].status.as_deref(), Some("Error"));
        assert!(
            document.rows[0]
                .details
                .iter()
                .any(|line| line.contains("failed_count"))
        );
        assert!(
            document.rows[0]
                .details
                .iter()
                .any(|line| line == "strict gate failed")
        );
    }
}
