use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Widget;

use crate::history_cell::ChatLane;
use crate::history_cell::HistoryCell;
use crate::history_presentation::PatchCellId;
use crate::line_truncation::line_width;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::workspace::theme as workspace_theme;

const CHAT_TIMELINE_SIDE_PADDING: u16 = 2;
const CHAT_TIMELINE_CARD_INNER_PADDING: u16 = 1;
const CHAT_TIMELINE_CARD_BORDER_COLS: u16 = 2;
const CHAT_TIMELINE_CARD_EXTRA_WIDTH: u16 =
    CHAT_TIMELINE_CARD_BORDER_COLS + CHAT_TIMELINE_CARD_INNER_PADDING * 2;
const CHAT_SURFACE_CONTENT_MAX_WIDTH: u16 = 96;
const CHAT_TIMELINE_USER_MAX_WIDTH: u16 = 56;
const CHAT_TIMELINE_ASSISTANT_MAX_WIDTH: u16 = 96;
const CHAT_TIMELINE_USER_WIDTH_PERCENT: u16 = 58;

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceTranscriptTail {
    pub(crate) lane: ChatLane,
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) patch_cell_id: Option<PatchCellId>,
}

pub(crate) struct WorkspaceTranscriptRequest<'a> {
    pub(crate) content_area: Rect,
    pub(crate) transcript_cells: &'a [Arc<dyn HistoryCell>],
    pub(crate) scroll_from_bottom: usize,
    pub(crate) theme: workspace_theme::WorkspaceTheme,
    pub(crate) theme_kind: workspace_theme::WorkspaceThemeKind,
    pub(crate) presentation_revision: u64,
    pub(crate) active_tail: Option<WorkspaceTranscriptTail>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceTranscriptViewport {
    pub(crate) content_area: Rect,
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) patch_cell_ids: Vec<PatchCellId>,
}

#[derive(Default)]
pub(crate) struct WorkspaceTranscriptCache {
    committed_layout: Option<WorkspaceTranscriptCommittedLayout>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkspaceTranscriptCommittedLayoutKey {
    width: u16,
    theme_kind: workspace_theme::WorkspaceThemeKind,
    committed_len: usize,
    committed_first_ptr: usize,
    committed_last_ptr: usize,
    presentation_revision: u64,
}

#[derive(Clone, Debug)]
struct WorkspaceTranscriptCommittedLayout {
    key: WorkspaceTranscriptCommittedLayoutKey,
    rows: Vec<WorkspaceTranscriptRow>,
    blocks: Vec<WorkspaceTranscriptBlock>,
}

#[derive(Clone, Debug)]
struct WorkspaceTranscriptBlock {
    lane: ChatLane,
    lines: Vec<Line<'static>>,
    patch_cell_ids: Vec<PatchCellId>,
    row_start: usize,
}

#[derive(Clone, Debug)]
struct WorkspaceTranscriptRow {
    line: Line<'static>,
    patch_cell_ids: Vec<PatchCellId>,
}

impl WorkspaceTranscriptCache {
    pub(crate) fn scroll_limit(&mut self, request: WorkspaceTranscriptRequest<'_>) -> usize {
        let visible_rows = usize::from(request.content_area.height);
        let committed_layout = self.committed_layout_for_request(&request);
        let total_rows = total_rows_for_request(&request, committed_layout);
        total_rows.saturating_sub(visible_rows)
    }

    pub(crate) fn viewport(
        &mut self,
        request: WorkspaceTranscriptRequest<'_>,
    ) -> WorkspaceTranscriptViewport {
        let content_area = request.content_area;
        let visible_rows = usize::from(content_area.height);
        let scroll_from_bottom = request.scroll_from_bottom;
        let committed_layout = self.committed_layout_for_request(&request);
        let segments = row_segments_for_request(&request, committed_layout);
        let total_rows = segments
            .iter()
            .map(WorkspaceTranscriptRowSegment::len)
            .sum();

        if visible_rows == 0 || total_rows == 0 {
            return WorkspaceTranscriptViewport {
                content_area,
                lines: Vec::new(),
                patch_cell_ids: Vec::new(),
            };
        }

        let skipped = scroll_from_bottom.min(total_rows);
        let end = total_rows.saturating_sub(skipped);
        let start = end.saturating_sub(visible_rows);
        let visible = visible_rows_from_segments(&segments, start, end);
        let mut patch_cell_ids = Vec::new();
        for row in &visible {
            for id in &row.patch_cell_ids {
                if !patch_cell_ids.contains(id) {
                    patch_cell_ids.push(*id);
                }
            }
        }

        WorkspaceTranscriptViewport {
            content_area,
            lines: visible.into_iter().map(|row| row.line).collect(),
            patch_cell_ids,
        }
    }

    fn committed_layout_for_request(
        &mut self,
        request: &WorkspaceTranscriptRequest<'_>,
    ) -> &WorkspaceTranscriptCommittedLayout {
        let key = WorkspaceTranscriptCommittedLayoutKey {
            width: request.content_area.width,
            theme_kind: request.theme_kind,
            committed_len: request.transcript_cells.len(),
            committed_first_ptr: request
                .transcript_cells
                .first()
                .map(history_cell_ptr_id)
                .unwrap_or_default(),
            committed_last_ptr: request
                .transcript_cells
                .last()
                .map(history_cell_ptr_id)
                .unwrap_or_default(),
            presentation_revision: request.presentation_revision,
        };
        let needs_refresh = self
            .committed_layout
            .as_ref()
            .is_none_or(|layout| layout.key != key);
        if needs_refresh {
            self.committed_layout = Some(WorkspaceTranscriptCommittedLayout {
                key,
                rows: Vec::new(),
                blocks: Vec::new(),
            });
            let layout = self
                .committed_layout
                .as_mut()
                .expect("workspace transcript committed layout should be populated");
            build_committed_layout(request, layout);
        }

        self.committed_layout
            .as_ref()
            .expect("workspace transcript committed layout cache should be populated")
    }
}

pub(crate) fn render_viewport(viewport: &WorkspaceTranscriptViewport, buf: &mut Buffer) {
    if viewport.lines.is_empty() {
        return;
    }

    let visible_height = usize::from(viewport.content_area.height);
    let rendered_len = visible_height.min(viewport.lines.len());
    let top_offset = visible_height.saturating_sub(rendered_len);
    for visible_index in 0..rendered_len {
        let Some(source_line) = viewport.lines.get(visible_index) else {
            break;
        };
        let y_offset = top_offset.saturating_add(visible_index);
        source_line.clone().render(
            Rect::new(
                viewport.content_area.x,
                viewport
                    .content_area
                    .y
                    .saturating_add(u16::try_from(y_offset).unwrap_or(u16::MAX)),
                viewport.content_area.width,
                1,
            ),
            buf,
        );
    }
}

pub(crate) fn lane_width(width: u16, lane: ChatLane) -> u16 {
    let (_, frame_width) = chat_timeline_frame(width);
    let available = frame_width
        .saturating_sub(CHAT_TIMELINE_SIDE_PADDING.saturating_mul(2))
        .saturating_sub(CHAT_TIMELINE_CARD_EXTRA_WIDTH)
        .max(1);
    match lane {
        ChatLane::Assistant => available.min(CHAT_TIMELINE_ASSISTANT_MAX_WIDTH).max(1),
        ChatLane::User => {
            let proportional = frame_width.saturating_mul(CHAT_TIMELINE_USER_WIDTH_PERCENT) / 100;
            proportional
                .min(CHAT_TIMELINE_USER_MAX_WIDTH)
                .min(available)
                .max(1)
        }
    }
}

pub(crate) fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    if lines.is_empty() {
        Vec::new()
    } else {
        crate::wrapping::word_wrap_lines(lines, usize::from(width.max(1)))
    }
}

enum WorkspaceTranscriptRowSegment<'a> {
    Borrowed(&'a [WorkspaceTranscriptRow]),
    Owned(Vec<WorkspaceTranscriptRow>),
}

impl WorkspaceTranscriptRowSegment<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Borrowed(rows) => rows.len(),
            Self::Owned(rows) => rows.len(),
        }
    }

    fn visible_rows(&self, start: usize, end: usize) -> Vec<WorkspaceTranscriptRow> {
        match self {
            Self::Borrowed(rows) => rows[start..end].to_vec(),
            Self::Owned(rows) => rows[start..end].to_vec(),
        }
    }
}

fn total_rows_for_request(
    request: &WorkspaceTranscriptRequest<'_>,
    committed_layout: &WorkspaceTranscriptCommittedLayout,
) -> usize {
    row_segments_for_request(request, committed_layout)
        .iter()
        .map(WorkspaceTranscriptRowSegment::len)
        .sum()
}

fn row_segments_for_request<'a>(
    request: &WorkspaceTranscriptRequest<'_>,
    committed_layout: &'a WorkspaceTranscriptCommittedLayout,
) -> Vec<WorkspaceTranscriptRowSegment<'a>> {
    let Some(tail) = request.active_tail.as_ref() else {
        return vec![WorkspaceTranscriptRowSegment::Borrowed(
            &committed_layout.rows,
        )];
    };

    if let Some(last_block) = committed_layout.blocks.last()
        && matches!(tail.lane, ChatLane::Assistant)
        && matches!(last_block.lane, ChatLane::Assistant)
    {
        let mut segments = Vec::with_capacity(2);
        if last_block.row_start > 0 {
            segments.push(WorkspaceTranscriptRowSegment::Borrowed(
                &committed_layout.rows[..last_block.row_start],
            ));
        }
        segments.push(WorkspaceTranscriptRowSegment::Owned(merged_tail_rows(
            request.content_area.width,
            request.theme,
            last_block,
            tail,
        )));
        return segments;
    }

    let mut segments = Vec::with_capacity(3);
    if !committed_layout.rows.is_empty() {
        segments.push(WorkspaceTranscriptRowSegment::Borrowed(
            &committed_layout.rows,
        ));
    }
    let tail_rows = active_tail_rows(request.content_area.width, request.theme, tail);
    if !tail_rows.is_empty() {
        if !committed_layout.rows.is_empty() {
            segments.push(WorkspaceTranscriptRowSegment::Owned(vec![blank_row()]));
        }
        segments.push(WorkspaceTranscriptRowSegment::Owned(tail_rows));
    }
    segments
}

fn visible_rows_from_segments(
    segments: &[WorkspaceTranscriptRowSegment<'_>],
    start: usize,
    end: usize,
) -> Vec<WorkspaceTranscriptRow> {
    let mut rows = Vec::with_capacity(end.saturating_sub(start));
    let mut segment_start = 0usize;
    for segment in segments {
        let segment_end = segment_start.saturating_add(segment.len());
        let visible_start = start.max(segment_start);
        let visible_end = end.min(segment_end);
        if visible_start < visible_end {
            rows.extend(segment.visible_rows(
                visible_start.saturating_sub(segment_start),
                visible_end.saturating_sub(segment_start),
            ));
        }
        segment_start = segment_end;
        if segment_start >= end {
            break;
        }
    }
    rows
}

fn build_committed_layout(
    request: &WorkspaceTranscriptRequest<'_>,
    layout: &mut WorkspaceTranscriptCommittedLayout,
) {
    let width = request.content_area.width;
    let mut content_blocks: Vec<(ChatLane, Vec<Line<'static>>, Vec<PatchCellId>)> = Vec::new();

    for cell in request.transcript_cells {
        let lane = cell.chat_lane();
        let lane_width = lane_width(width, lane);
        let lines = wrap_lines(cell.committed_display_lines(lane_width), lane_width);
        push_chat_timeline_content_block(
            &mut content_blocks,
            lane,
            lines,
            cell.patch_cell_id(),
            !cell.is_stream_continuation(),
        );
    }

    layout.rows.clear();
    layout.blocks.clear();
    for (index, (lane, lines, patch_cell_ids)) in content_blocks.into_iter().enumerate() {
        if index > 0 {
            layout.rows.push(blank_row());
        }
        let row_start = layout.rows.len();
        let rendered_rows =
            rows_for_block(width, request.theme, lane, lines.clone(), &patch_cell_ids);
        layout.rows.extend(rendered_rows);
        layout.blocks.push(WorkspaceTranscriptBlock {
            lane,
            lines,
            patch_cell_ids,
            row_start,
        });
    }
}

fn merged_tail_rows(
    width: u16,
    theme: workspace_theme::WorkspaceTheme,
    block: &WorkspaceTranscriptBlock,
    tail: &WorkspaceTranscriptTail,
) -> Vec<WorkspaceTranscriptRow> {
    let mut lines = block.lines.clone();
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.extend(tail.lines.clone());
    let mut patch_cell_ids = block.patch_cell_ids.clone();
    if let Some(id) = tail.patch_cell_id
        && !patch_cell_ids.contains(&id)
    {
        patch_cell_ids.push(id);
    }
    rows_for_block(width, theme, block.lane, lines, &patch_cell_ids)
}

fn active_tail_rows(
    width: u16,
    theme: workspace_theme::WorkspaceTheme,
    tail: &WorkspaceTranscriptTail,
) -> Vec<WorkspaceTranscriptRow> {
    let patch_cell_ids = tail.patch_cell_id.into_iter().collect::<Vec<_>>();
    rows_for_block(width, theme, tail.lane, tail.lines.clone(), &patch_cell_ids)
}

fn rows_for_block(
    width: u16,
    theme: workspace_theme::WorkspaceTheme,
    lane: ChatLane,
    lines: Vec<Line<'static>>,
    patch_cell_ids: &[PatchCellId],
) -> Vec<WorkspaceTranscriptRow> {
    chat_timeline_lines_for_raw(width, lane, lines, theme)
        .into_iter()
        .map(|line| WorkspaceTranscriptRow {
            line,
            patch_cell_ids: patch_cell_ids.to_vec(),
        })
        .collect()
}

fn blank_row() -> WorkspaceTranscriptRow {
    WorkspaceTranscriptRow {
        line: Line::from(""),
        patch_cell_ids: Vec::new(),
    }
}

fn push_chat_timeline_content_block(
    blocks: &mut Vec<(ChatLane, Vec<Line<'static>>, Vec<PatchCellId>)>,
    lane: ChatLane,
    mut lines: Vec<Line<'static>>,
    patch_cell_id: Option<PatchCellId>,
    separate_from_previous: bool,
) {
    if lines.is_empty() {
        return;
    }

    if matches!(lane, ChatLane::Assistant)
        && let Some((last_lane, last_lines, last_patch_cell_ids)) = blocks.last_mut()
        && *last_lane == lane
    {
        if separate_from_previous && !last_lines.is_empty() {
            last_lines.push(Line::from(""));
        }
        last_lines.append(&mut lines);
        if let Some(id) = patch_cell_id
            && !last_patch_cell_ids.contains(&id)
        {
            last_patch_cell_ids.push(id);
        }
    } else {
        let patch_cell_ids = patch_cell_id.into_iter().collect();
        blocks.push((lane, lines, patch_cell_ids));
    }
}

fn chat_timeline_lines_for_raw(
    width: u16,
    lane: ChatLane,
    lines: Vec<Line<'static>>,
    theme: workspace_theme::WorkspaceTheme,
) -> Vec<Line<'static>> {
    let lane_width = lane_width(width, lane);
    let wrapped = wrap_lines(lines, lane_width);
    chat_timeline_card_rows(width, lane, lane_width, wrapped, theme)
}

fn chat_timeline_frame(width: u16) -> (u16, u16) {
    let frame_width = chat_surface_column_width(width);
    let left_offset = width.saturating_sub(frame_width) / 2;
    (left_offset, frame_width)
}

fn chat_surface_column_width(width: u16) -> u16 {
    if width == 0 {
        0
    } else {
        width.min(CHAT_SURFACE_CONTENT_MAX_WIDTH).max(1)
    }
}

fn chat_timeline_card_rows(
    width: u16,
    lane: ChatLane,
    lane_width: u16,
    lines: Vec<Line<'static>>,
    theme: workspace_theme::WorkspaceTheme,
) -> Vec<Line<'static>> {
    if width == 0 || lines.is_empty() {
        return Vec::new();
    }

    let content_width = lines
        .iter()
        .map(line_width)
        .max()
        .unwrap_or(1)
        .max(1)
        .min(usize::from(lane_width)) as u16;
    let card_width = content_width
        .saturating_add(CHAT_TIMELINE_CARD_EXTRA_WIDTH)
        .max(4);
    let left_pad = chat_timeline_card_left_pad(width, lane, card_width);
    let card_bg = Color::Rgb(0, 0, 0);
    let border_bg = theme.panel_raised_bg;
    let border_fg = theme.border_muted;
    let text_fg = match lane {
        ChatLane::Assistant => theme.text,
        ChatLane::User => theme.text_strong,
    };

    let mut rows = Vec::with_capacity(lines.len().saturating_add(2));
    rows.push(chat_timeline_card_border_row(
        left_pad, card_width, border_bg, border_fg, true,
    ));
    for line in lines {
        rows.push(chat_timeline_card_body_row(
            left_pad,
            content_width,
            line,
            card_bg,
            border_bg,
            border_fg,
            text_fg,
        ));
    }
    rows.push(chat_timeline_card_border_row(
        left_pad, card_width, border_bg, border_fg, false,
    ));
    rows
}

fn chat_timeline_card_left_pad(width: u16, lane: ChatLane, card_width: u16) -> u16 {
    let (frame_left, frame_width) = chat_timeline_frame(width);
    let side_pad = CHAT_TIMELINE_SIDE_PADDING.min(frame_width.saturating_sub(card_width));
    match lane {
        ChatLane::Assistant => frame_left.saturating_add(side_pad),
        ChatLane::User => frame_left
            .saturating_add(frame_width.saturating_sub(card_width))
            .saturating_sub(side_pad),
    }
}

fn chat_timeline_card_border_row(
    left_pad: u16,
    card_width: u16,
    card_bg: Color,
    border_fg: Color,
    top: bool,
) -> Line<'static> {
    let border_style = Style::default().fg(border_fg).bg(card_bg);
    let fill_count = card_width.saturating_sub(2) as usize;
    let (left, right) = if top { ("╭", "╮") } else { ("╰", "╯") };

    let mut spans = chat_timeline_leading_spans(left_pad);
    spans.push(Span::styled(left, border_style));
    if fill_count > 0 {
        spans.push(Span::styled("─".repeat(fill_count), border_style));
    }
    spans.push(Span::styled(right, border_style));
    Line::from(spans)
}

fn chat_timeline_card_body_row(
    left_pad: u16,
    content_width: u16,
    line: Line<'static>,
    card_bg: Color,
    border_bg: Color,
    border_fg: Color,
    text_fg: Color,
) -> Line<'static> {
    let border_style = Style::default().fg(border_fg).bg(border_bg);
    let body_style = Style::default().fg(text_fg).bg(card_bg);
    let mut line = truncate_line_with_ellipsis_if_overflow(line, usize::from(content_width));
    let used_width = u16::try_from(line_width(&line)).unwrap_or(u16::MAX);
    let fill_width = content_width.saturating_sub(used_width);
    for span in &mut line.spans {
        if span.style.fg.is_none() {
            span.style = span.style.fg(text_fg);
        }
        span.style = span.style.bg(card_bg);
    }

    let mut spans = chat_timeline_leading_spans(left_pad);
    spans.push(Span::styled("│", border_style));
    spans.push(Span::styled(
        " ".repeat(CHAT_TIMELINE_CARD_INNER_PADDING as usize),
        body_style,
    ));
    spans.extend(line.spans);
    if fill_width > 0 {
        spans.push(Span::styled(" ".repeat(fill_width as usize), body_style));
    }
    spans.push(Span::styled(
        " ".repeat(CHAT_TIMELINE_CARD_INNER_PADDING as usize),
        body_style,
    ));
    spans.push(Span::styled("│", border_style));
    Line::from(spans)
}

fn chat_timeline_leading_spans(left_pad: u16) -> Vec<Span<'static>> {
    if left_pad == 0 {
        Vec::new()
    } else {
        vec![Span::raw(" ".repeat(left_pad as usize))]
    }
}

fn history_cell_ptr_id(cell: &Arc<dyn HistoryCell>) -> usize {
    Arc::as_ptr(cell) as *const () as usize
}
