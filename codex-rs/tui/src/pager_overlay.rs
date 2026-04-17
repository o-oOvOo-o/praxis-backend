//! Overlay UIs rendered in an alternate screen.
//!
//! This module implements the pager-style overlays used by the TUI, including the transcript
//! overlay (`Ctrl+T`) that renders a full history view separate from the main viewport.
//!
//! The transcript overlay renders committed transcript cells plus an optional render-only live tail
//! derived from the current in-flight active cell. Because rebuilding wrapped `Line`s on every draw
//! can be expensive, that live tail is cached and only recomputed when its cache key changes, which
//! is derived from the terminal width (wrapping), an active-cell revision (in-place mutations), the
//! stream-continuation flag (spacing), and an animation tick (time-based spinner/shimmer output).
//!
//! The transcript overlay live tail is kept in sync by `App` during draws: `App` supplies an
//! `ActiveCellTranscriptKey` and a function to compute the active cell transcript lines, and
//! `TranscriptOverlay::sync_live_tail` uses the key to decide when the cached tail must be
//! recomputed. `ChatWidget` is responsible for producing a key that changes when the active cell
//! mutates in place or when its transcript output is time-dependent.

use std::io::Result;
use std::sync::Arc;

use crate::chatwidget::ActiveCellTranscriptKey;
use crate::history_cell::HistoryCell;
use crate::history_cell::UserHistoryCell;
use crate::key_hint;
use crate::key_hint::KeyBinding;
use crate::render::Insets;
use crate::render::renderable::InsetRenderable;
use crate::render::renderable::Renderable;
use crate::style::search_highlight_overlay;
use crate::style::user_message_style;
use crate::transcript_search::TranscriptSearchOverlayState;
use crate::transcript_search::TranscriptSearchStatus;
use crate::tui;
use crate::tui::TuiEvent;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;

pub(crate) enum Overlay {
    Transcript(TranscriptOverlay),
    Static(StaticOverlay),
}

impl Overlay {
    pub(crate) fn new_transcript(cells: Vec<Arc<dyn HistoryCell>>) -> Self {
        Self::Transcript(TranscriptOverlay::new(cells))
    }

    pub(crate) fn new_static_with_lines(lines: Vec<Line<'static>>, title: String) -> Self {
        Self::Static(StaticOverlay::with_title(lines, title))
    }

    pub(crate) fn new_static_with_renderables(
        renderables: Vec<Box<dyn Renderable>>,
        title: String,
    ) -> Self {
        Self::Static(StaticOverlay::with_renderables(renderables, title))
    }

    pub(crate) fn handle_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        match self {
            Overlay::Transcript(o) => o.handle_event(tui, event),
            Overlay::Static(o) => o.handle_event(tui, event),
        }
    }

    pub(crate) fn is_done(&self) -> bool {
        match self {
            Overlay::Transcript(o) => o.is_done(),
            Overlay::Static(o) => o.is_done(),
        }
    }
}

const KEY_UP: KeyBinding = key_hint::plain(KeyCode::Up);
const KEY_DOWN: KeyBinding = key_hint::plain(KeyCode::Down);
const KEY_K: KeyBinding = key_hint::plain(KeyCode::Char('k'));
const KEY_J: KeyBinding = key_hint::plain(KeyCode::Char('j'));
const KEY_PAGE_UP: KeyBinding = key_hint::plain(KeyCode::PageUp);
const KEY_PAGE_DOWN: KeyBinding = key_hint::plain(KeyCode::PageDown);
const KEY_SPACE: KeyBinding = key_hint::plain(KeyCode::Char(' '));
const KEY_SHIFT_SPACE: KeyBinding = key_hint::shift(KeyCode::Char(' '));
const KEY_HOME: KeyBinding = key_hint::plain(KeyCode::Home);
const KEY_END: KeyBinding = key_hint::plain(KeyCode::End);
const KEY_LEFT: KeyBinding = key_hint::plain(KeyCode::Left);
const KEY_RIGHT: KeyBinding = key_hint::plain(KeyCode::Right);
const KEY_CTRL_F: KeyBinding = key_hint::ctrl(KeyCode::Char('f'));
const KEY_CTRL_D: KeyBinding = key_hint::ctrl(KeyCode::Char('d'));
const KEY_CTRL_B: KeyBinding = key_hint::ctrl(KeyCode::Char('b'));
const KEY_CTRL_U: KeyBinding = key_hint::ctrl(KeyCode::Char('u'));
const KEY_Q: KeyBinding = key_hint::plain(KeyCode::Char('q'));
const KEY_ESC: KeyBinding = key_hint::plain(KeyCode::Esc);
const KEY_ENTER: KeyBinding = key_hint::plain(KeyCode::Enter);
const KEY_CTRL_T: KeyBinding = key_hint::ctrl(KeyCode::Char('t'));
const KEY_CTRL_C: KeyBinding = key_hint::ctrl(KeyCode::Char('c'));
const MOUSE_WHEEL_SCROLL_LINES: usize = 3;

// Common pager navigation hints rendered on the first line
const PAGER_KEY_HINTS: &[(&[KeyBinding], &str)] = &[
    (&[KEY_UP, KEY_DOWN], "to scroll"),
    (&[KEY_PAGE_UP, KEY_PAGE_DOWN], "to page"),
    (&[KEY_HOME, KEY_END], "to jump"),
];

// Render a single line of key hints from (key(s), description) pairs.
fn render_key_hints(area: Rect, buf: &mut Buffer, pairs: &[(&[KeyBinding], &str)]) {
    let mut spans: Vec<Span<'static>> = vec![" ".into()];
    let mut first = true;
    for (keys, desc) in pairs {
        if !first {
            spans.push("   ".into());
        }
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                spans.push("/".into());
            }
            spans.push(Span::from(key));
        }
        spans.push(" ".into());
        spans.push(Span::from(desc.to_string()));
        first = false;
    }
    Paragraph::new(vec![Line::from(spans).dim()]).render_ref(area, buf);
}

/// Generic widget for rendering a pager view.
struct PagerView {
    renderables: Vec<Box<dyn Renderable>>,
    scroll_offset: usize,
    title: String,
    last_content_height: Option<usize>,
    last_rendered_height: Option<usize>,
    /// If set, on next render ensure this chunk is visible.
    pending_scroll_chunk: Option<usize>,
}

impl PagerView {
    fn new(renderables: Vec<Box<dyn Renderable>>, title: String, scroll_offset: usize) -> Self {
        Self {
            renderables,
            scroll_offset,
            title,
            last_content_height: None,
            last_rendered_height: None,
            pending_scroll_chunk: None,
        }
    }

    fn content_height(&self, width: u16) -> usize {
        self.renderables
            .iter()
            .map(|c| c.desired_height(width) as usize)
            .sum()
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        self.render_header(area, buf);
        let content_area = self.content_area(area);
        self.update_last_content_height(content_area.height);
        let content_height = self.content_height(content_area.width);
        self.last_rendered_height = Some(content_height);
        // If there is a pending request to scroll a specific chunk into view,
        // satisfy it now that wrapping is up to date for this width.
        if let Some(idx) = self.pending_scroll_chunk.take() {
            self.ensure_chunk_visible(idx, content_area);
        }
        self.scroll_offset = self
            .scroll_offset
            .min(content_height.saturating_sub(content_area.height as usize));

        self.render_content(content_area, buf);

        self.render_bottom_bar(area, content_area, buf, content_height);
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        Span::from("/ ".repeat(area.width as usize / 2))
            .dim()
            .render_ref(area, buf);
        let header = format!("/ {}", self.title);
        header.dim().render_ref(area, buf);
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let mut y = -(self.scroll_offset as isize);
        let mut drawn_bottom = area.y;
        for renderable in &self.renderables {
            let top = y;
            let height = renderable.desired_height(area.width) as isize;
            y += height;
            let bottom = y;
            if bottom < area.y as isize {
                continue;
            }
            if top > area.y as isize + area.height as isize {
                break;
            }
            if top < 0 {
                let drawn = render_offset_content(area, buf, &**renderable, (-top) as u16);
                drawn_bottom = drawn_bottom.max(area.y + drawn);
            } else {
                let draw_height = (height as u16).min(area.height.saturating_sub(top as u16));
                let draw_area = Rect::new(area.x, area.y + top as u16, area.width, draw_height);
                renderable.render(draw_area, buf);
                drawn_bottom = drawn_bottom.max(draw_area.y.saturating_add(draw_area.height));
            }
        }

        for y in drawn_bottom..area.bottom() {
            if area.width == 0 {
                break;
            }
            buf[(area.x, y)] = Cell::from('~');
            for x in area.x + 1..area.right() {
                buf[(x, y)] = Cell::from(' ');
            }
        }
    }

    fn render_bottom_bar(
        &self,
        full_area: Rect,
        content_area: Rect,
        buf: &mut Buffer,
        total_len: usize,
    ) {
        let sep_y = content_area.bottom();
        let sep_rect = Rect::new(full_area.x, sep_y, full_area.width, 1);

        Span::from("─".repeat(sep_rect.width as usize))
            .dim()
            .render_ref(sep_rect, buf);
        let percent = if total_len == 0 {
            100
        } else {
            let max_scroll = total_len.saturating_sub(content_area.height as usize);
            if max_scroll == 0 {
                100
            } else {
                (((self.scroll_offset.min(max_scroll)) as f32 / max_scroll as f32) * 100.0).round()
                    as u8
            }
        };
        let pct_text = format!(" {percent}% ");
        let pct_w = pct_text.chars().count() as u16;
        let pct_x = sep_rect.x + sep_rect.width - pct_w - 1;
        Span::from(pct_text)
            .dim()
            .render_ref(Rect::new(pct_x, sep_rect.y, pct_w, 1), buf);
    }

    fn handle_key_event(&mut self, tui: &mut tui::Tui, key_event: KeyEvent) -> Result<()> {
        let previous_scroll_offset = self.scroll_offset;
        match key_event {
            e if KEY_UP.is_press(e) || KEY_K.is_press(e) => {
                self.scroll_up(1);
            }
            e if KEY_DOWN.is_press(e) || KEY_J.is_press(e) => {
                self.scroll_down(1);
            }
            e if KEY_PAGE_UP.is_press(e)
                || KEY_SHIFT_SPACE.is_press(e)
                || KEY_CTRL_B.is_press(e) =>
            {
                let page_height = self.page_height(tui.terminal.viewport_area);
                self.scroll_up(page_height);
            }
            e if KEY_PAGE_DOWN.is_press(e) || KEY_SPACE.is_press(e) || KEY_CTRL_F.is_press(e) => {
                let page_height = self.page_height(tui.terminal.viewport_area);
                self.scroll_down(page_height);
            }
            e if KEY_CTRL_D.is_press(e) => {
                let area = self.content_area(tui.terminal.viewport_area);
                let half_page = (area.height as usize).saturating_add(1) / 2;
                self.scroll_down(half_page);
            }
            e if KEY_CTRL_U.is_press(e) => {
                let area = self.content_area(tui.terminal.viewport_area);
                let half_page = (area.height as usize).saturating_add(1) / 2;
                self.scroll_up(half_page);
            }
            e if KEY_HOME.is_press(e) => {
                self.scroll_offset = 0;
            }
            e if KEY_END.is_press(e) => {
                self.scroll_offset = usize::MAX;
            }
            _ => {
                return Ok(());
            }
        }
        if self.scroll_offset != previous_scroll_offset {
            tui.frame_requester().schedule_scroll_frame();
        }
        Ok(())
    }

    fn handle_mouse_event(&mut self, mouse_event: MouseEvent) -> bool {
        let previous_scroll_offset = self.scroll_offset;
        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_up(MOUSE_WHEEL_SCROLL_LINES);
            }
            MouseEventKind::ScrollDown => {
                self.scroll_down(MOUSE_WHEEL_SCROLL_LINES);
            }
            _ => return false,
        }
        self.scroll_offset != previous_scroll_offset
    }

    /// Returns the height of one page in content rows.
    ///
    /// Prefers the last rendered content height (excluding header/footer chrome);
    /// if no render has occurred yet, falls back to the content area height
    /// computed from the given viewport.
    fn page_height(&self, viewport_area: Rect) -> usize {
        self.last_content_height
            .unwrap_or_else(|| self.content_area(viewport_area).height as usize)
    }

    fn update_last_content_height(&mut self, height: u16) {
        self.last_content_height = Some(height as usize);
    }

    fn content_area(&self, area: Rect) -> Rect {
        let mut area = area;
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(2);
        area
    }

    fn max_scroll_for_known_layout(&self) -> Option<usize> {
        let total_height = self.last_rendered_height?;
        let content_height = self.last_content_height?;
        Some(total_height.saturating_sub(content_height))
    }

    fn normalized_scroll_offset(&self) -> usize {
        self.max_scroll_for_known_layout()
            .map_or(self.scroll_offset, |max_scroll| {
                self.scroll_offset.min(max_scroll)
            })
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.normalized_scroll_offset().saturating_sub(amount);
    }

    fn scroll_down(&mut self, amount: usize) {
        let next = self.normalized_scroll_offset().saturating_add(amount);
        self.scroll_offset = self
            .max_scroll_for_known_layout()
            .map_or(next, |max_scroll| next.min(max_scroll));
    }
}

impl PagerView {
    fn is_scrolled_to_bottom(&self) -> bool {
        if self.scroll_offset == usize::MAX {
            return true;
        }
        let Some(height) = self.last_content_height else {
            return false;
        };
        if self.renderables.is_empty() {
            return true;
        }
        let Some(total_height) = self.last_rendered_height else {
            return false;
        };
        if total_height <= height {
            return true;
        }
        let max_scroll = total_height.saturating_sub(height);
        self.scroll_offset >= max_scroll
    }

    /// Request that the given text chunk index be scrolled into view on next render.
    fn scroll_chunk_into_view(&mut self, chunk_index: usize) {
        self.pending_scroll_chunk = Some(chunk_index);
    }

    fn ensure_chunk_visible(&mut self, idx: usize, area: Rect) {
        if area.height == 0 || idx >= self.renderables.len() {
            return;
        }
        let first = self
            .renderables
            .iter()
            .take(idx)
            .map(|r| r.desired_height(area.width) as usize)
            .sum();
        let last = first + self.renderables[idx].desired_height(area.width) as usize;
        let current_top = self.scroll_offset;
        let current_bottom = current_top.saturating_add(area.height.saturating_sub(1) as usize);
        if first < current_top {
            self.scroll_offset = first;
        } else if last > current_bottom {
            self.scroll_offset = last.saturating_sub(area.height.saturating_sub(1) as usize);
        }
    }
}

/// A renderable that caches its desired height.
struct CachedRenderable {
    renderable: Box<dyn Renderable>,
    height: std::cell::Cell<Option<u16>>,
    last_width: std::cell::Cell<Option<u16>>,
}

impl CachedRenderable {
    fn new(renderable: impl Into<Box<dyn Renderable>>) -> Self {
        Self {
            renderable: renderable.into(),
            height: std::cell::Cell::new(None),
            last_width: std::cell::Cell::new(None),
        }
    }
}

impl Renderable for CachedRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.renderable.render(area, buf);
    }
    fn desired_height(&self, width: u16) -> u16 {
        if self.last_width.get() != Some(width) {
            let height = self.renderable.desired_height(width);
            self.height.set(Some(height));
            self.last_width.set(Some(width));
        }
        self.height.get().unwrap_or(0)
    }
}

struct CellRenderable {
    cell: Arc<dyn HistoryCell>,
    style: Style,
    search_match: Option<SearchChunkHighlight>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchMatchHighlight {
    chunk_index: usize,
    query: String,
    line_index: usize,
    match_index_in_line: usize,
}

impl SearchMatchHighlight {
    fn from_status(status: &TranscriptSearchStatus) -> Option<Self> {
        let target = status.current_target?;
        (!status.query.is_empty()).then(|| Self {
            chunk_index: target.chunk_index,
            query: status.query.clone(),
            line_index: target.line_index,
            match_index_in_line: target.match_index_in_line,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchChunkHighlight {
    query: String,
    current_match: Option<SearchMatchHighlight>,
}

impl SearchChunkHighlight {
    fn from_status(status: &TranscriptSearchStatus, chunk_index: usize) -> Option<Self> {
        (!status.query.is_empty()).then(|| Self {
            query: status.query.clone(),
            current_match: SearchMatchHighlight::from_status(status)
                .filter(|search_match| search_match.chunk_index == chunk_index),
        })
    }
}

struct TranscriptLinesRenderable {
    lines: Vec<Line<'static>>,
    search_match: Option<SearchChunkHighlight>,
}

fn stylize_transcript_lines(
    lines: Vec<Line<'static>>,
    base_style: Style,
    search_match: Option<&SearchChunkHighlight>,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .enumerate()
        .map(|(line_index, line)| {
            let line = materialize_line_style(line, base_style);
            let current_match_index = search_match.and_then(|search_match| {
                search_match
                    .current_match
                    .as_ref()
                    .filter(|current_match| current_match.line_index == line_index)
                    .map(|current_match| current_match.match_index_in_line)
            });
            highlight_line_matches(
                line,
                search_match.map(|search_match| search_match.query.as_str()),
                current_match_index,
            )
        })
        .collect()
}

fn materialize_line_style(line: Line<'static>, base_style: Style) -> Line<'static> {
    let Line {
        spans,
        style,
        alignment,
    } = line;
    let merged_line_style = style.patch(base_style);
    if spans.is_empty() {
        return Line {
            spans,
            style: merged_line_style,
            alignment,
        };
    }

    let spans = spans
        .into_iter()
        .map(|span| Span {
            style: span.style.patch(merged_line_style),
            content: span.content,
        })
        .collect();

    Line {
        spans,
        style: Style::default(),
        alignment,
    }
}

fn highlight_line_matches(
    line: Line<'static>,
    query: Option<&str>,
    current_match_index: Option<usize>,
) -> Line<'static> {
    let Some(query) = query else {
        return line;
    };
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let target_ranges = line_search_match_ranges(&text, query);
    if target_ranges.is_empty() {
        return line;
    }

    let Line {
        spans,
        style,
        alignment,
    } = line;
    let mut out = Vec::with_capacity(spans.len().saturating_add(target_ranges.len() * 2));
    let mut cursor = 0usize;

    for span in spans {
        let content = span.content.into_owned();
        let span_start = cursor;
        let span_end = span_start + content.len();
        cursor = span_end;

        let intersecting_ranges = target_ranges
            .iter()
            .enumerate()
            .filter(|(_, target_range)| {
                target_range.start < span_end && target_range.end > span_start
            })
            .collect::<Vec<_>>();
        if intersecting_ranges.is_empty() {
            out.push(Span::styled(content, span.style));
            continue;
        }

        let mut local_cursor = 0usize;
        for (range_index, target_range) in intersecting_ranges {
            let local_start = target_range.start.saturating_sub(span_start);
            let local_end = target_range.end.min(span_end).saturating_sub(span_start);

            if local_start > local_cursor {
                out.push(Span::styled(
                    content[local_cursor..local_start].to_string(),
                    span.style,
                ));
            }
            if local_end > local_start {
                let highlight_style = if current_match_index == Some(range_index) {
                    search_highlight_overlay(span.style).add_modifier(Modifier::REVERSED)
                } else {
                    search_highlight_overlay(span.style)
                };
                out.push(Span::styled(
                    content[local_start..local_end].to_string(),
                    highlight_style,
                ));
            }
            local_cursor = local_end;
        }
        if local_cursor < content.len() {
            out.push(Span::styled(
                content[local_cursor..].to_string(),
                span.style,
            ));
        }
    }

    Line {
        spans: out,
        style,
        alignment,
    }
}

fn line_search_match_ranges(line: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let normalized_query = query.to_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let mut normalized_line = String::new();
    let mut normalized_byte_to_char_index = Vec::new();
    let mut original_char_ranges = Vec::new();
    for (char_index, (orig_start, ch)) in line.char_indices().enumerate() {
        let orig_end = orig_start + ch.len_utf8();
        original_char_ranges.push(orig_start..orig_end);
        let lower = ch.to_lowercase().collect::<String>();
        normalized_byte_to_char_index.extend(std::iter::repeat(char_index).take(lower.len()));
        normalized_line.push_str(&lower);
    }

    let mut ranges = Vec::new();
    let mut offset = 0usize;
    while let Some(relative) = normalized_line[offset..].find(&normalized_query) {
        let normalized_start = offset + relative;
        let normalized_end = normalized_start + normalized_query.len();
        let start_char_index = normalized_byte_to_char_index[normalized_start];
        let end_char_index = normalized_byte_to_char_index[normalized_end - 1];
        ranges.push(
            original_char_ranges[start_char_index].start..original_char_ranges[end_char_index].end,
        );
        offset = normalized_end;
    }

    ranges
}

impl Renderable for CellRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = stylize_transcript_lines(
            self.cell.transcript_lines(area.width),
            self.style,
            self.search_match.as_ref(),
        );
        let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        p.render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.cell.desired_transcript_height(width)
    }
}

impl Renderable for TranscriptLinesRenderable {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = stylize_transcript_lines(
            self.lines.clone(),
            Style::default(),
            self.search_match.as_ref(),
        );
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        if let [line] = &self.lines[..]
            && line
                .spans
                .iter()
                .all(|span| span.content.chars().all(char::is_whitespace))
        {
            return 1;
        }

        Paragraph::new(Text::from(self.lines.clone()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }
}

pub(crate) struct TranscriptOverlay {
    /// Pager UI state and the renderables currently displayed.
    ///
    /// The invariant is that `view.renderables` is `render_cells(cells)` plus an optional trailing
    /// live-tail renderable appended after the committed cells.
    view: PagerView,
    /// Committed transcript cells (does not include the live tail).
    cells: Vec<Arc<dyn HistoryCell>>,
    highlight_cell: Option<usize>,
    search_status: Option<TranscriptSearchStatus>,
    search_target_chunk: Option<usize>,
    search_highlight_cell: Option<usize>,
    search_match_highlight: Option<TranscriptSearchStatus>,
    /// Cache key for the render-only live tail appended after committed cells.
    live_tail_key: Option<LiveTailKey>,
    live_tail_lines: Option<Vec<Line<'static>>>,
    is_done: bool,
}

/// Cache key for the active-cell "live tail" appended to the transcript overlay.
///
/// Changing any field implies a different rendered tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LiveTailKey {
    /// Current terminal width, which affects wrapping.
    width: u16,
    /// Revision that changes on in-place active cell transcript updates.
    revision: u64,
    /// Revision that changes when history fold/expand presentation changes globally.
    presentation_revision: u64,
    /// Whether the tail should be treated as a continuation for spacing.
    is_stream_continuation: bool,
    /// Optional animation tick to refresh spinners/progress indicators.
    animation_tick: Option<u64>,
}

impl TranscriptOverlay {
    /// Creates a transcript overlay for a fixed set of committed cells.
    ///
    /// This overlay does not own the "active cell"; callers may optionally append a live tail via
    /// `sync_live_tail` during draws to reflect in-flight activity.
    pub(crate) fn new(transcript_cells: Vec<Arc<dyn HistoryCell>>) -> Self {
        Self {
            view: PagerView::new(
                Self::render_cells(
                    &transcript_cells,
                    /*highlight_cell*/ None,
                    /*search_match*/ None,
                ),
                "T R A N S C R I P T".to_string(),
                usize::MAX,
            ),
            cells: transcript_cells,
            highlight_cell: None,
            search_status: None,
            search_target_chunk: None,
            search_highlight_cell: None,
            search_match_highlight: None,
            live_tail_key: None,
            live_tail_lines: None,
            is_done: false,
        }
    }

    fn render_cells(
        cells: &[Arc<dyn HistoryCell>],
        highlight_cell: Option<usize>,
        search_match: Option<&TranscriptSearchStatus>,
    ) -> Vec<Box<dyn Renderable>> {
        cells
            .iter()
            .enumerate()
            .flat_map(|(i, c)| {
                let mut v: Vec<Box<dyn Renderable>> = Vec::new();
                let is_highlighted = highlight_cell == Some(i);
                let search_match = search_match
                    .and_then(|search_match| SearchChunkHighlight::from_status(search_match, i));
                let mut cell_renderable = if c.as_any().is::<UserHistoryCell>() {
                    Box::new(CachedRenderable::new(CellRenderable {
                        cell: c.clone(),
                        style: if is_highlighted {
                            user_message_style().reversed()
                        } else {
                            user_message_style()
                        },
                        search_match: search_match.clone(),
                    })) as Box<dyn Renderable>
                } else {
                    Box::new(CachedRenderable::new(CellRenderable {
                        cell: c.clone(),
                        style: if is_highlighted {
                            Style::default().reversed()
                        } else {
                            Style::default()
                        },
                        search_match: search_match.clone(),
                    })) as Box<dyn Renderable>
                };
                if !c.is_stream_continuation() && i > 0 {
                    cell_renderable = Box::new(InsetRenderable::new(
                        cell_renderable,
                        Insets::tlbr(
                            /*top*/ 1, /*left*/ 0, /*bottom*/ 0, /*right*/ 0,
                        ),
                    ));
                }
                v.push(cell_renderable);
                v
            })
            .collect()
    }

    /// Insert a committed history cell while keeping any cached live tail.
    ///
    /// The live tail is temporarily removed, the committed cells are rebuilt,
    /// then the tail is reattached. If the tail previously had no leading
    /// spacing because it was the only renderable, we add the missing inset
    /// when the first committed cell arrives.
    ///
    /// This expects `cell` to be a committed transcript cell (not the in-flight active cell). If
    /// the overlay was scrolled to bottom before insertion, it remains pinned to bottom after the
    /// insertion to preserve the "follow along" behavior.
    pub(crate) fn insert_cell(&mut self, cell: Arc<dyn HistoryCell>) {
        let follow_bottom = self.view.is_scrolled_to_bottom();
        self.cells.push(cell);
        self.rebuild_renderables();
        if follow_bottom {
            self.view.scroll_offset = usize::MAX;
        }
    }

    /// Replace committed transcript cells while keeping any cached in-progress output that is
    /// currently shown at the end of the overlay.
    ///
    /// This is used when existing history is trimmed (for example after rollback) so the
    /// transcript overlay immediately reflects the same committed cells as the main transcript.
    pub(crate) fn replace_cells(&mut self, cells: Vec<Arc<dyn HistoryCell>>) {
        let follow_bottom = self.view.is_scrolled_to_bottom();
        self.cells = cells;
        if self
            .effective_highlight_cell()
            .is_some_and(|idx| idx >= self.cells.len())
        {
            self.highlight_cell = None;
            self.search_highlight_cell = None;
        }
        self.rebuild_renderables();
        if follow_bottom {
            self.view.scroll_offset = usize::MAX;
        }
    }

    /// Sync the active-cell live tail with the current width and cell state.
    ///
    /// Recomputes the tail only when the cache key changes, preserving scroll
    /// position and dropping the tail if there is nothing to render.
    ///
    /// The overlay owns committed transcript cells while the live tail is derived from the current
    /// active cell, which can mutate in place while streaming. `App` calls this during
    /// `TuiEvent::Draw` for `Overlay::Transcript`, passing a key that changes when the active cell
    /// mutates or animates so the cached tail stays fresh.
    ///
    /// Passing a key that does not change on in-place active-cell mutations will freeze the tail in
    /// `Ctrl+T` while the main viewport continues to update.
    pub(crate) fn sync_live_tail(
        &mut self,
        width: u16,
        active_key: Option<ActiveCellTranscriptKey>,
        compute_lines: impl FnOnce(u16) -> Option<Vec<Line<'static>>>,
    ) {
        let next_key = active_key.map(|key| LiveTailKey {
            width,
            revision: key.revision,
            presentation_revision: key.presentation_revision,
            is_stream_continuation: key.is_stream_continuation,
            animation_tick: key.animation_tick,
        });

        if self.live_tail_key == next_key {
            return;
        }
        let follow_bottom = self.view.is_scrolled_to_bottom();

        self.live_tail_key = next_key;
        self.live_tail_lines = next_key.and_then(|_| {
            let lines = compute_lines(width).unwrap_or_default();
            (!lines.is_empty()).then_some(lines)
        });
        self.rebuild_renderables();
        if follow_bottom {
            self.view.scroll_offset = usize::MAX;
        }
    }

    pub(crate) fn set_highlight_cell(&mut self, cell: Option<usize>) {
        self.highlight_cell = cell;
        self.rebuild_renderables();
        if let Some(idx) = self.effective_highlight_cell() {
            self.view.scroll_chunk_into_view(idx);
        }
    }

    pub(crate) fn set_search_state(&mut self, state: Option<TranscriptSearchOverlayState>) {
        self.search_status = state.as_ref().map(|state| state.status.clone());
        self.search_target_chunk = state.as_ref().and_then(|state| state.current_chunk);
        self.search_highlight_cell = state.as_ref().and_then(|state| state.highlight_cell);
        self.search_match_highlight = self.search_status.clone();
        self.rebuild_renderables();
        if let Some(chunk_index) = self
            .search_target_chunk
            .or_else(|| self.effective_highlight_cell())
        {
            self.view.scroll_chunk_into_view(chunk_index);
        }
    }

    /// Returns whether the underlying pager view is currently pinned to the bottom.
    ///
    /// The `App` draw loop uses this to decide whether to schedule animation frames for the live
    /// tail; if the user has scrolled up, we avoid driving animation work that they cannot see.
    pub(crate) fn is_scrolled_to_bottom(&self) -> bool {
        self.view.is_scrolled_to_bottom()
    }

    fn rebuild_renderables(&mut self) {
        self.view.renderables = Self::render_cells(
            &self.cells,
            self.effective_highlight_cell(),
            self.search_match_highlight.as_ref(),
        );
        if let Some(lines) = &self.live_tail_lines {
            self.view.renderables.push(Self::live_tail_renderable(
                lines.clone(),
                !self.cells.is_empty(),
                self.live_tail_key
                    .is_some_and(|key| key.is_stream_continuation),
                self.search_match_highlight
                    .as_ref()
                    .and_then(|search_match| {
                        SearchChunkHighlight::from_status(search_match, self.cells.len())
                    }),
            ));
        }
    }

    fn effective_highlight_cell(&self) -> Option<usize> {
        self.highlight_cell.or(self.search_highlight_cell)
    }

    fn live_tail_renderable(
        lines: Vec<Line<'static>>,
        has_prior_cells: bool,
        is_stream_continuation: bool,
        search_match: Option<SearchChunkHighlight>,
    ) -> Box<dyn Renderable> {
        let mut renderable: Box<dyn Renderable> =
            Box::new(CachedRenderable::new(TranscriptLinesRenderable {
                lines,
                search_match,
            }));
        if has_prior_cells && !is_stream_continuation {
            renderable = Box::new(InsetRenderable::new(
                renderable,
                Insets::tlbr(
                    /*top*/ 1, /*left*/ 0, /*bottom*/ 0, /*right*/ 0,
                ),
            ));
        }
        renderable
    }

    fn render_hints(&self, area: Rect, buf: &mut Buffer) {
        let line1 = Rect::new(area.x, area.y, area.width, 1);
        let line2 = Rect::new(area.x, area.y.saturating_add(1), area.width, 1);
        let line3 = Rect::new(area.x, area.y.saturating_add(2), area.width, 1);
        render_key_hints(line1, buf, PAGER_KEY_HINTS);

        let mut pairs: Vec<(&[KeyBinding], &str)> = vec![(&[KEY_Q], "to quit")];
        if self.effective_highlight_cell().is_some() {
            pairs.push((&[KEY_ESC, KEY_LEFT], "to edit prev"));
            pairs.push((&[KEY_RIGHT], "to edit next"));
            pairs.push((&[KEY_ENTER], "to edit message"));
        } else {
            pairs.push((&[KEY_ESC], "to edit prev"));
        }
        if let Some(search_status) = &self.search_status {
            Paragraph::new(vec![Line::from(search_status.render_text()).dim()])
                .render_ref(line2, buf);
            render_key_hints(line3, buf, &pairs);
        } else {
            render_key_hints(line2, buf, &pairs);
        }
    }

    pub(crate) fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let top_h = area.height.saturating_sub(3);
        let top = Rect::new(area.x, area.y, area.width, top_h);
        let bottom = Rect::new(area.x, area.y + top_h, area.width, 3);
        self.view.render(top, buf);
        self.render_hints(bottom, buf);
    }
}

impl TranscriptOverlay {
    pub(crate) fn handle_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        match event {
            TuiEvent::Key(key_event) => match key_event {
                e if KEY_Q.is_press(e) || KEY_CTRL_C.is_press(e) || KEY_CTRL_T.is_press(e) => {
                    self.is_done = true;
                    Ok(())
                }
                other => self.view.handle_key_event(tui, other),
            },
            TuiEvent::Mouse(mouse_event) => {
                if self.view.handle_mouse_event(mouse_event) {
                    tui.frame_requester().schedule_scroll_frame();
                }
                Ok(())
            }
            TuiEvent::Draw => {
                tui.draw(u16::MAX, |frame| {
                    self.render(frame.area(), frame.buffer);
                })?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn is_done(&self) -> bool {
        self.is_done
    }

    #[cfg(test)]
    pub(crate) fn committed_cell_count(&self) -> usize {
        self.cells.len()
    }
}

pub(crate) struct StaticOverlay {
    view: PagerView,
    is_done: bool,
}

impl StaticOverlay {
    pub(crate) fn with_title(lines: Vec<Line<'static>>, title: String) -> Self {
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        Self::with_renderables(vec![Box::new(CachedRenderable::new(paragraph))], title)
    }

    pub(crate) fn with_renderables(renderables: Vec<Box<dyn Renderable>>, title: String) -> Self {
        Self {
            view: PagerView::new(renderables, title, /*scroll_offset*/ 0),
            is_done: false,
        }
    }

    fn render_hints(&self, area: Rect, buf: &mut Buffer) {
        let line1 = Rect::new(area.x, area.y, area.width, 1);
        let line2 = Rect::new(area.x, area.y.saturating_add(1), area.width, 1);
        render_key_hints(line1, buf, PAGER_KEY_HINTS);
        let pairs: Vec<(&[KeyBinding], &str)> = vec![(&[KEY_Q], "to quit")];
        render_key_hints(line2, buf, &pairs);
    }

    pub(crate) fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let top_h = area.height.saturating_sub(3);
        let top = Rect::new(area.x, area.y, area.width, top_h);
        let bottom = Rect::new(area.x, area.y + top_h, area.width, 3);
        self.view.render(top, buf);
        self.render_hints(bottom, buf);
    }
}

impl StaticOverlay {
    pub(crate) fn handle_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        match event {
            TuiEvent::Key(key_event) => match key_event {
                e if KEY_Q.is_press(e) || KEY_CTRL_C.is_press(e) => {
                    self.is_done = true;
                    Ok(())
                }
                other => self.view.handle_key_event(tui, other),
            },
            TuiEvent::Mouse(mouse_event) => {
                if self.view.handle_mouse_event(mouse_event) {
                    tui.frame_requester().schedule_scroll_frame();
                }
                Ok(())
            }
            TuiEvent::Draw => {
                tui.draw(u16::MAX, |frame| {
                    self.render(frame.area(), frame.buffer);
                })?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn is_done(&self) -> bool {
        self.is_done
    }
}

fn render_offset_content(
    area: Rect,
    buf: &mut Buffer,
    renderable: &dyn Renderable,
    scroll_offset: u16,
) -> u16 {
    let height = renderable.desired_height(area.width);
    let mut tall_buf = Buffer::empty(Rect::new(
        0,
        0,
        area.width,
        height.min(area.height + scroll_offset),
    ));
    renderable.render(*tall_buf.area(), &mut tall_buf);
    let copy_height = area
        .height
        .min(tall_buf.area().height.saturating_sub(scroll_offset));
    for y in 0..copy_height {
        let src_y = y + scroll_offset;
        for x in 0..area.width {
            buf[(area.x + x, area.y + y)] = tall_buf[(x, src_y)].clone();
        }
    }

    copy_height
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::protocol::ExecCommandSource;
    use codex_protocol::protocol::ReviewDecision;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::exec_cell::CommandOutput;
    use crate::history_cell;
    use crate::history_cell::HistoryCell;
    use crate::history_cell::new_patch_event;
    use codex_protocol::parse_command::ParsedCommand;
    use codex_protocol::protocol::FileChange;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::text::Text;

    #[derive(Debug)]
    struct TestCell {
        lines: Vec<Line<'static>>,
    }

    impl crate::history_cell::HistoryCell for TestCell {
        fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
            self.lines.clone()
        }

        fn transcript_lines(&self, _width: u16) -> Vec<Line<'static>> {
            self.lines.clone()
        }
    }

    fn paragraph_block(label: &str, lines: usize) -> Box<dyn Renderable> {
        let text = Text::from(
            (0..lines)
                .map(|i| Line::from(format!("{label}{i}")))
                .collect::<Vec<_>>(),
        );
        Box::new(Paragraph::new(text)) as Box<dyn Renderable>
    }

    #[test]
    fn edit_prev_hint_is_visible() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("hello")],
        })]);

        // Render into a wide buffer so the footer hints aren't truncated.
        let area = Rect::new(0, 0, 120, 10);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let s = buffer_to_text(&buf, area);
        assert!(
            s.contains("edit prev"),
            "expected 'edit prev' hint in overlay footer, got: {s:?}"
        );
    }

    #[test]
    fn edit_next_hint_is_visible_when_highlighted() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("hello")],
        })]);
        overlay.set_highlight_cell(Some(0));

        // Render into a wide buffer so the footer hints aren't truncated.
        let area = Rect::new(0, 0, 120, 10);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let s = buffer_to_text(&buf, area);
        assert!(
            s.contains("edit next"),
            "expected 'edit next' hint in overlay footer, got: {s:?}"
        );
    }

    #[test]
    fn transcript_overlay_snapshot_basic() {
        // Prepare a transcript overlay with a few lines
        let mut overlay = TranscriptOverlay::new(vec![
            Arc::new(TestCell {
                lines: vec![Line::from("alpha")],
            }),
            Arc::new(TestCell {
                lines: vec![Line::from("beta")],
            }),
            Arc::new(TestCell {
                lines: vec![Line::from("gamma")],
            }),
        ]);
        let mut term = Terminal::new(TestBackend::new(40, 10)).expect("term");
        term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(term.backend());
    }

    #[test]
    fn transcript_overlay_renders_live_tail() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("alpha")],
        })]);
        overlay.sync_live_tail(
            /*width*/ 40,
            Some(ActiveCellTranscriptKey {
                revision: 1,
                is_stream_continuation: false,
                animation_tick: None,
                presentation_revision: 0,
            }),
            |_| Some(vec![Line::from("tail")]),
        );

        let mut term = Terminal::new(TestBackend::new(40, 10)).expect("term");
        term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(term.backend());
    }

    #[test]
    fn transcript_overlay_sync_live_tail_is_noop_for_identical_key() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("alpha")],
        })]);

        let calls = std::cell::Cell::new(0usize);
        let key = ActiveCellTranscriptKey {
            revision: 1,
            is_stream_continuation: false,
            animation_tick: None,
            presentation_revision: 0,
        };

        overlay.sync_live_tail(/*width*/ 40, Some(key), |_| {
            calls.set(calls.get() + 1);
            Some(vec![Line::from("tail")])
        });
        overlay.sync_live_tail(/*width*/ 40, Some(key), |_| {
            calls.set(calls.get() + 1);
            Some(vec![Line::from("tail2")])
        });

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn transcript_overlay_renders_search_status_line() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("alpha")],
        })]);
        overlay.set_search_state(Some(TranscriptSearchOverlayState {
            status: TranscriptSearchStatus {
                query: "alpha".to_string(),
                result_count: 1,
                current_ordinal: Some(1),
                current_target: None,
                wrapped: false,
            },
            current_chunk: Some(0),
            highlight_cell: Some(0),
        }));

        let area = Rect::new(0, 0, 120, 10);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let s = buffer_to_text(&buf, area);
        assert!(
            s.contains("Search: alpha  1/1"),
            "expected transcript search status in overlay footer, got: {s:?}"
        );
    }

    #[test]
    fn transcript_overlay_highlights_current_search_match_in_committed_cell() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from(vec![
                Span::raw("alpha "),
                Span::raw("be"),
                Span::raw("ta gamma"),
            ])],
        })]);
        overlay.set_search_state(Some(TranscriptSearchOverlayState {
            status: TranscriptSearchStatus {
                query: "beta".to_string(),
                result_count: 1,
                current_ordinal: Some(1),
                current_target: Some(crate::transcript_search::TranscriptSearchTarget {
                    chunk_index: 0,
                    cell_index: Some(0),
                    line_index: 0,
                    match_index_in_line: 0,
                    is_live_tail: false,
                }),
                wrapped: false,
            },
            current_chunk: Some(0),
            highlight_cell: None,
        }));

        let area = Rect::new(0, 0, 80, 10);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let content_area = overlay.view.content_area(Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(3),
        ));
        assert_text_has_search_highlight(&buf, content_area, "beta");
    }

    #[test]
    fn transcript_overlay_highlights_current_search_match_in_live_tail() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("alpha")],
        })]);
        overlay.sync_live_tail(
            /*width*/ 80,
            Some(ActiveCellTranscriptKey {
                revision: 1,
                is_stream_continuation: false,
                animation_tick: None,
                presentation_revision: 0,
            }),
            |_| Some(vec![Line::from("tail beta")]),
        );
        overlay.set_search_state(Some(TranscriptSearchOverlayState {
            status: TranscriptSearchStatus {
                query: "beta".to_string(),
                result_count: 1,
                current_ordinal: Some(1),
                current_target: Some(crate::transcript_search::TranscriptSearchTarget {
                    chunk_index: 1,
                    cell_index: None,
                    line_index: 0,
                    match_index_in_line: 0,
                    is_live_tail: true,
                }),
                wrapped: false,
            },
            current_chunk: Some(1),
            highlight_cell: None,
        }));

        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let content_area = overlay.view.content_area(Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(3),
        ));
        assert_text_has_search_highlight(&buf, content_area, "beta");
    }

    #[test]
    fn transcript_overlay_highlights_all_search_matches_and_emphasizes_current_one() {
        let mut overlay = TranscriptOverlay::new(vec![Arc::new(TestCell {
            lines: vec![Line::from("beta beta")],
        })]);
        overlay.set_search_state(Some(TranscriptSearchOverlayState {
            status: TranscriptSearchStatus {
                query: "beta".to_string(),
                result_count: 2,
                current_ordinal: Some(2),
                current_target: Some(crate::transcript_search::TranscriptSearchTarget {
                    chunk_index: 0,
                    cell_index: Some(0),
                    line_index: 0,
                    match_index_in_line: 1,
                    is_live_tail: false,
                }),
                wrapped: false,
            },
            current_chunk: Some(0),
            highlight_cell: None,
        }));

        let area = Rect::new(0, 0, 80, 10);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let content_area = overlay.view.content_area(Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(3),
        ));
        let expected_bg = crate::style::search_highlight_style()
            .bg
            .expect("search highlight style should set a background");
        let (y, row) = rendered_row_containing(&buf, content_area, "beta beta")
            .expect("expected rendered row with repeated match");
        let first_col = row.find("beta").expect("first beta");
        let second_col = row[first_col + 4..]
            .find("beta")
            .map(|col| first_col + 4 + col)
            .expect("second beta");

        for offset in 0..4u16 {
            assert_eq!(
                buf[(content_area.x + first_col as u16 + offset, y)].bg,
                expected_bg
            );
            assert_eq!(
                buf[(content_area.x + second_col as u16 + offset, y)].bg,
                expected_bg
            );
        }
        assert!(
            buf[(content_area.x + second_col as u16, y)]
                .modifier
                .contains(Modifier::REVERSED),
            "expected current search match to carry extra emphasis"
        );
        assert!(
            !buf[(content_area.x + first_col as u16, y)]
                .modifier
                .contains(Modifier::REVERSED),
            "expected non-current matches to omit current-match emphasis"
        );
    }

    fn buffer_to_text(buf: &Buffer, area: Rect) -> String {
        let mut out = String::new();
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let symbol = buf[(x, y)].symbol();
                if symbol.is_empty() {
                    out.push(' ');
                } else {
                    out.push(symbol.chars().next().unwrap_or(' '));
                }
            }
            // Trim trailing spaces for stability.
            while out.ends_with(' ') {
                out.pop();
            }
            out.push('\n');
        }
        out
    }

    fn assert_text_has_search_highlight(buf: &Buffer, area: Rect, needle: &str) {
        let expected_bg = crate::style::search_highlight_style()
            .bg
            .expect("search highlight style should set a background");

        for y in area.y..area.bottom() {
            let mut row = String::new();
            for x in area.x..area.right() {
                row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            if let Some(col) = row.find(needle) {
                for offset in 0..needle.len() as u16 {
                    assert_eq!(
                        buf[(area.x + col as u16 + offset, y)].bg,
                        expected_bg,
                        "expected highlighted background for {needle:?} at ({}, {})",
                        area.x + col as u16 + offset,
                        y
                    );
                }
                return;
            }
        }

        panic!(
            "did not find {needle:?} in rendered area: {:?}",
            buffer_to_text(buf, area)
        );
    }

    fn rendered_row_containing(buf: &Buffer, area: Rect, needle: &str) -> Option<(u16, String)> {
        for y in area.y..area.bottom() {
            let mut row = String::new();
            for x in area.x..area.right() {
                row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            if row.contains(needle) {
                return Some((y, row));
            }
        }
        None
    }

    #[test]
    fn transcript_overlay_apply_patch_scroll_vt100_clears_previous_page() {
        let cwd = PathBuf::from("/repo");
        let mut cells: Vec<Arc<dyn HistoryCell>> = Vec::new();

        let mut approval_changes = HashMap::new();
        approval_changes.insert(
            PathBuf::from("foo.txt"),
            FileChange::Add {
                content: "hello\nworld\n".to_string(),
            },
        );
        let approval_cell: Arc<dyn HistoryCell> = Arc::new(new_patch_event(approval_changes, &cwd));
        cells.push(approval_cell);

        let mut apply_changes = HashMap::new();
        apply_changes.insert(
            PathBuf::from("foo.txt"),
            FileChange::Add {
                content: "hello\nworld\n".to_string(),
            },
        );
        let apply_begin_cell: Arc<dyn HistoryCell> = Arc::new(new_patch_event(apply_changes, &cwd));
        cells.push(apply_begin_cell);

        let apply_end_cell: Arc<dyn HistoryCell> = history_cell::new_approval_decision_cell(
            vec!["ls".into()],
            ReviewDecision::Approved,
            history_cell::ApprovalDecisionActor::User,
        )
        .into();
        cells.push(apply_end_cell);

        let mut exec_cell = crate::exec_cell::new_active_exec_command(
            "exec-1".into(),
            vec!["bash".into(), "-lc".into(), "ls".into()],
            vec![ParsedCommand::Unknown { cmd: "ls".into() }],
            ExecCommandSource::Agent,
            /*interaction_input*/ None,
            /*animations_enabled*/ true,
        );
        exec_cell.complete_call(
            "exec-1",
            CommandOutput {
                exit_code: 0,
                aggregated_output: "src\nREADME.md\n".into(),
                formatted_output: "src\nREADME.md\n".into(),
            },
            Duration::from_millis(420),
        );
        let exec_cell: Arc<dyn HistoryCell> = Arc::new(exec_cell);
        cells.push(exec_cell);

        let mut overlay = TranscriptOverlay::new(cells);
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);

        overlay.render(area, &mut buf);
        overlay.view.scroll_offset = 0;
        overlay.render(area, &mut buf);

        let snapshot = buffer_to_text(&buf, area);
        assert_snapshot!("transcript_overlay_apply_patch_scroll_vt100", snapshot);
    }

    #[test]
    fn transcript_overlay_keeps_scroll_pinned_at_bottom() {
        let mut overlay = TranscriptOverlay::new(
            (0..20)
                .map(|i| {
                    Arc::new(TestCell {
                        lines: vec![Line::from(format!("line{i}"))],
                    }) as Arc<dyn HistoryCell>
                })
                .collect(),
        );
        let mut term = Terminal::new(TestBackend::new(40, 12)).expect("term");
        term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
            .expect("draw");

        assert!(
            overlay.view.is_scrolled_to_bottom(),
            "expected initial render to leave view at bottom"
        );

        overlay.insert_cell(Arc::new(TestCell {
            lines: vec!["tail".into()],
        }));

        assert_eq!(overlay.view.scroll_offset, usize::MAX);
    }

    #[test]
    fn transcript_overlay_preserves_manual_scroll_position() {
        let mut overlay = TranscriptOverlay::new(
            (0..20)
                .map(|i| {
                    Arc::new(TestCell {
                        lines: vec![Line::from(format!("line{i}"))],
                    }) as Arc<dyn HistoryCell>
                })
                .collect(),
        );
        let mut term = Terminal::new(TestBackend::new(40, 12)).expect("term");
        term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
            .expect("draw");

        overlay.view.scroll_offset = 0;

        overlay.insert_cell(Arc::new(TestCell {
            lines: vec!["tail".into()],
        }));

        assert_eq!(overlay.view.scroll_offset, 0);
    }

    #[test]
    fn static_overlay_snapshot_basic() {
        // Prepare a static overlay with a few lines and a title
        let mut overlay = StaticOverlay::with_title(
            vec!["one".into(), "two".into(), "three".into()],
            "S T A T I C".to_string(),
        );
        let mut term = Terminal::new(TestBackend::new(40, 10)).expect("term");
        term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(term.backend());
    }

    /// Render transcript overlay and return visible line numbers (`line-NN`) in order.
    fn transcript_line_numbers(overlay: &mut TranscriptOverlay, area: Rect) -> Vec<usize> {
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        let top_h = area.height.saturating_sub(3);
        let top = Rect::new(area.x, area.y, area.width, top_h);
        let content_area = overlay.view.content_area(top);

        let mut nums = Vec::new();
        for y in content_area.y..content_area.bottom() {
            let mut line = String::new();
            for x in content_area.x..content_area.right() {
                line.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            if let Some(n) = line
                .split_whitespace()
                .find_map(|w| w.strip_prefix("line-"))
                .and_then(|s| s.parse().ok())
            {
                nums.push(n);
            }
        }
        nums
    }

    #[test]
    fn transcript_overlay_paging_is_continuous_and_round_trips() {
        let mut overlay = TranscriptOverlay::new(
            (0..50)
                .map(|i| {
                    Arc::new(TestCell {
                        lines: vec![Line::from(format!("line-{i:02}"))],
                    }) as Arc<dyn HistoryCell>
                })
                .collect(),
        );
        let area = Rect::new(0, 0, 40, 15);

        // Prime layout so last_content_height is populated and paging uses the real content height.
        let mut buf = Buffer::empty(area);
        overlay.view.scroll_offset = 0;
        overlay.render(area, &mut buf);
        let page_height = overlay.view.page_height(area);

        // Scenario 1: starting from the top, PageDown should show the next page of content.
        overlay.view.scroll_offset = 0;
        let page1 = transcript_line_numbers(&mut overlay, area);
        let page1_len = page1.len();
        let expected_page1: Vec<usize> = (0..page1_len).collect();
        assert_eq!(
            page1, expected_page1,
            "first page should start at line-00 and show a full page of content"
        );

        overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_add(page_height);
        let page2 = transcript_line_numbers(&mut overlay, area);
        assert_eq!(
            page2.len(),
            page1_len,
            "second page should have the same number of visible lines as the first page"
        );
        let expected_page2_first = *page1.last().unwrap() + 1;
        assert_eq!(
            page2[0], expected_page2_first,
            "second page after PageDown should immediately follow the first page"
        );

        // Scenario 2: from an interior offset (start=3), PageDown then PageUp should round-trip.
        let interior_offset = 3usize;
        overlay.view.scroll_offset = interior_offset;
        let before = transcript_line_numbers(&mut overlay, area);
        overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_add(page_height);
        let _ = transcript_line_numbers(&mut overlay, area);
        overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_sub(page_height);
        let after = transcript_line_numbers(&mut overlay, area);
        assert_eq!(
            before, after,
            "PageDown+PageUp from interior offset ({interior_offset}) should round-trip"
        );

        // Scenario 3: from the top of the second page, PageUp then PageDown should round-trip.
        overlay.view.scroll_offset = page_height;
        let before2 = transcript_line_numbers(&mut overlay, area);
        overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_sub(page_height);
        let _ = transcript_line_numbers(&mut overlay, area);
        overlay.view.scroll_offset = overlay.view.scroll_offset.saturating_add(page_height);
        let after2 = transcript_line_numbers(&mut overlay, area);
        assert_eq!(
            before2, after2,
            "PageUp+PageDown from the top of the second page should round-trip"
        );
    }

    #[test]
    fn static_overlay_wraps_long_lines() {
        let mut overlay = StaticOverlay::with_title(
            vec!["a very long line that should wrap when rendered within a narrow pager overlay width".into()],
            "S T A T I C".to_string(),
        );
        let mut term = Terminal::new(TestBackend::new(24, 8)).expect("term");
        term.draw(|f| overlay.render(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(term.backend());
    }

    #[test]
    fn pager_view_content_height_counts_renderables() {
        let pv = PagerView::new(
            vec![
                paragraph_block("a", /*lines*/ 2),
                paragraph_block("b", /*lines*/ 3),
            ],
            "T".to_string(),
            /*scroll_offset*/ 0,
        );

        assert_eq!(pv.content_height(/*width*/ 80), 5);
    }

    fn mouse_event(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }
    }

    #[test]
    fn pager_view_ensure_chunk_visible_scrolls_down_when_needed() {
        let mut pv = PagerView::new(
            vec![
                paragraph_block("a", /*lines*/ 1),
                paragraph_block("b", /*lines*/ 3),
                paragraph_block("c", /*lines*/ 3),
            ],
            "T".to_string(),
            /*scroll_offset*/ 0,
        );
        let area = Rect::new(0, 0, 20, 8);

        pv.scroll_offset = 0;
        let content_area = pv.content_area(area);
        pv.ensure_chunk_visible(/*idx*/ 2, content_area);

        let mut buf = Buffer::empty(area);
        pv.render(area, &mut buf);
        let rendered = buffer_to_text(&buf, area);

        assert!(
            rendered.contains("c0"),
            "expected chunk top in view: {rendered:?}"
        );
        assert!(
            rendered.contains("c1"),
            "expected chunk middle in view: {rendered:?}"
        );
        assert!(
            rendered.contains("c2"),
            "expected chunk bottom in view: {rendered:?}"
        );
    }

    #[test]
    fn pager_view_ensure_chunk_visible_scrolls_up_when_needed() {
        let mut pv = PagerView::new(
            vec![
                paragraph_block("a", /*lines*/ 2),
                paragraph_block("b", /*lines*/ 3),
                paragraph_block("c", /*lines*/ 3),
            ],
            "T".to_string(),
            /*scroll_offset*/ 0,
        );
        let area = Rect::new(0, 0, 20, 3);

        pv.scroll_offset = 6;
        pv.ensure_chunk_visible(/*idx*/ 0, area);

        assert_eq!(pv.scroll_offset, 0);
    }

    #[test]
    fn pager_view_is_scrolled_to_bottom_accounts_for_wrapped_height() {
        let mut pv = PagerView::new(
            vec![paragraph_block("a", /*lines*/ 10)],
            "T".to_string(),
            /*scroll_offset*/ 0,
        );
        let area = Rect::new(0, 0, 20, 8);
        let mut buf = Buffer::empty(area);

        pv.render(area, &mut buf);

        assert!(
            !pv.is_scrolled_to_bottom(),
            "expected view to report not at bottom when offset < max"
        );

        pv.scroll_offset = usize::MAX;
        pv.render(area, &mut buf);

        assert!(
            pv.is_scrolled_to_bottom(),
            "expected view to report at bottom after scrolling to end"
        );
    }

    #[test]
    fn pager_view_mouse_wheel_scrolls_by_three_lines() {
        let mut pv = PagerView::new(
            (0..20)
                .map(|i| paragraph_block(&format!("line-{i:02}-"), /*lines*/ 1))
                .collect(),
            "T".to_string(),
            /*scroll_offset*/ 0,
        );

        assert!(pv.handle_mouse_event(mouse_event(MouseEventKind::ScrollDown)));
        assert_eq!(pv.scroll_offset, 3);

        assert!(pv.handle_mouse_event(mouse_event(MouseEventKind::ScrollUp)));
        assert_eq!(pv.scroll_offset, 0);
    }

    #[test]
    fn pager_view_scroll_up_from_bottom_uses_concrete_bottom_offset() {
        let mut pv = PagerView::new(
            (0..20)
                .map(|i| paragraph_block(&format!("line-{i:02}-"), /*lines*/ 1))
                .collect(),
            "T".to_string(),
            /*scroll_offset*/ 0,
        );
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        pv.render(area, &mut buf);

        let max_scroll = pv
            .max_scroll_for_known_layout()
            .expect("render should populate layout heights");
        pv.scroll_offset = usize::MAX;
        assert!(pv.handle_mouse_event(mouse_event(MouseEventKind::ScrollUp)));

        assert_eq!(pv.scroll_offset, max_scroll.saturating_sub(3));
    }

    #[test]
    fn pager_view_ignores_non_scroll_mouse_events() {
        let mut pv = PagerView::new(
            vec![paragraph_block("a", /*lines*/ 4)],
            "T".to_string(),
            /*scroll_offset*/ 2,
        );

        assert!(!pv.handle_mouse_event(mouse_event(MouseEventKind::Moved)));
        assert_eq!(pv.scroll_offset, 2);
    }
}
