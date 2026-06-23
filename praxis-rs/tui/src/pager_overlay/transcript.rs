use super::pager_view::*;
use super::renderables::*;
use super::*;

pub(super) struct TranscriptLiveTail {
    lines: Vec<Line<'static>>,
    is_stream_continuation: bool,
}

pub(super) struct TranscriptPagerContent {
    cells: Vec<Arc<dyn HistoryCell>>,
    highlight_cell: Option<usize>,
    search_match: Option<TranscriptSearchStatus>,
    live_tail: Option<TranscriptLiveTail>,
}

impl TranscriptPagerContent {
    fn new(cells: Vec<Arc<dyn HistoryCell>>) -> Self {
        Self {
            cells,
            highlight_cell: None,
            search_match: None,
            live_tail: None,
        }
    }

    fn len(&self) -> usize {
        self.cells.len() + usize::from(self.live_tail.is_some())
    }

    fn is_live_tail_index(&self, idx: usize) -> bool {
        idx == self.cells.len() && self.live_tail.is_some()
    }

    fn desired_height(&self, idx: usize, width: u16) -> u16 {
        if let Some(cell) = self.cells.get(idx) {
            return height_with_optional_top_inset(
                cell.desired_transcript_height(width),
                idx > 0 && !cell.is_stream_continuation(),
            );
        }

        if self.is_live_tail_index(idx)
            && let Some(live_tail) = &self.live_tail
        {
            return height_with_optional_top_inset(
                transcript_lines_desired_height(&live_tail.lines, width),
                !self.cells.is_empty() && !live_tail.is_stream_continuation,
            );
        }

        0
    }

    fn render(&self, idx: usize, area: Rect, buf: &mut Buffer) {
        if let Some(renderable) = self.renderable(idx) {
            renderable.render(area, buf);
        }
    }

    fn render_window(&self, idx: usize, area: Rect, buf: &mut Buffer, scroll_offset: u16) {
        if let Some(renderable) = self.renderable(idx) {
            renderable.render_window(area, buf, scroll_offset);
        }
    }

    fn renderable(&self, idx: usize) -> Option<Box<dyn Renderable>> {
        if let Some(cell) = self.cells.get(idx) {
            return Some(transcript_cell_renderable(
                Arc::clone(cell),
                idx,
                self.highlight_cell,
                self.search_match.as_ref(),
            ));
        }

        if self.is_live_tail_index(idx)
            && let Some(live_tail) = &self.live_tail
        {
            return Some(transcript_live_tail_renderable(
                live_tail.lines.clone(),
                !self.cells.is_empty(),
                live_tail.is_stream_continuation,
                self.search_match.as_ref(),
                self.cells.len(),
            ));
        }

        None
    }
}

impl PagerContent for Rc<RefCell<TranscriptPagerContent>> {
    fn len(&self) -> usize {
        self.borrow().len()
    }

    fn desired_height(&self, idx: usize, width: u16) -> u16 {
        self.borrow().desired_height(idx, width)
    }

    fn render(&self, idx: usize, area: Rect, buf: &mut Buffer) {
        self.borrow().render(idx, area, buf);
    }

    fn render_window(&self, idx: usize, area: Rect, buf: &mut Buffer, scroll_offset: u16) {
        self.borrow().render_window(idx, area, buf, scroll_offset);
    }
}

pub(super) fn height_with_optional_top_inset(height: u16, has_top_inset: bool) -> u16 {
    height.saturating_add(u16::from(has_top_inset))
}

pub(super) fn transcript_lines_desired_height(lines: &[Line<'static>], width: u16) -> u16 {
    if let [line] = lines
        && line
            .spans
            .iter()
            .all(|span| span.content.chars().all(char::is_whitespace))
    {
        return 1;
    }

    Paragraph::new(Text::from(lines.to_vec()))
        .wrap(Wrap { trim: false })
        .line_count(width)
        .try_into()
        .unwrap_or(0)
}

pub(super) fn transcript_cell_renderable(
    cell: Arc<dyn HistoryCell>,
    idx: usize,
    highlight_cell: Option<usize>,
    search_match: Option<&TranscriptSearchStatus>,
) -> Box<dyn Renderable> {
    let is_highlighted = highlight_cell == Some(idx);
    let search_match =
        search_match.and_then(|search_match| SearchChunkHighlight::from_status(search_match, idx));
    let style = if cell.as_any().is::<UserHistoryCell>() {
        if is_highlighted {
            user_message_style().reversed()
        } else {
            user_message_style()
        }
    } else if is_highlighted {
        Style::default().reversed()
    } else {
        Style::default()
    };
    let mut renderable: Box<dyn Renderable> = Box::new(CellRenderable {
        cell: Arc::clone(&cell),
        style,
        search_match,
    });
    if !cell.is_stream_continuation() && idx > 0 {
        renderable = Box::new(InsetRenderable::new(
            renderable,
            Insets::tlbr(
                /*top*/ 1, /*left*/ 0, /*bottom*/ 0, /*right*/ 0,
            ),
        ));
    }
    renderable
}

pub(super) fn transcript_live_tail_renderable(
    lines: Vec<Line<'static>>,
    has_prior_cells: bool,
    is_stream_continuation: bool,
    search_match: Option<&TranscriptSearchStatus>,
    chunk_index: usize,
) -> Box<dyn Renderable> {
    let mut renderable: Box<dyn Renderable> = Box::new(TranscriptLinesRenderable {
        lines,
        search_match: search_match
            .and_then(|search_match| SearchChunkHighlight::from_status(search_match, chunk_index)),
    });
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

pub(crate) struct TranscriptOverlay {
    /// Pager UI state for the transcript list.
    view: PagerView,
    /// Shared lazy content source used by the pager.
    content: Rc<RefCell<TranscriptPagerContent>>,
    highlight_cell: Option<usize>,
    search_status: Option<TranscriptSearchStatus>,
    search_target_chunk: Option<usize>,
    search_highlight_cell: Option<usize>,
    search_match_highlight: Option<TranscriptSearchStatus>,
    /// Cache key for the render-only live tail appended after committed cells.
    live_tail_key: Option<LiveTailKey>,
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
        let content = Rc::new(RefCell::new(TranscriptPagerContent::new(transcript_cells)));
        Self {
            view: PagerView::new_with_content(
                Box::new(Rc::clone(&content)),
                "T R A N S C R I P T".to_string(),
                usize::MAX,
            ),
            content,
            highlight_cell: None,
            search_status: None,
            search_target_chunk: None,
            search_highlight_cell: None,
            search_match_highlight: None,
            live_tail_key: None,
            is_done: false,
        }
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
        self.content.borrow_mut().cells.push(cell);
        self.view.invalidate_layout();
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
        let cell_count = cells.len();
        self.content.borrow_mut().cells = cells;
        if self
            .effective_highlight_cell()
            .is_some_and(|idx| idx >= cell_count)
        {
            self.highlight_cell = None;
            self.search_highlight_cell = None;
        }
        self.sync_content_presentation();
        self.view.invalidate_layout();
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
        let live_tail = next_key.and_then(|key| {
            let lines = compute_lines(width).unwrap_or_default();
            (!lines.is_empty()).then_some(TranscriptLiveTail {
                lines,
                is_stream_continuation: key.is_stream_continuation,
            })
        });
        self.content.borrow_mut().live_tail = live_tail;
        self.view.invalidate_layout();
        if follow_bottom {
            self.view.scroll_offset = usize::MAX;
        }
    }

    pub(crate) fn set_highlight_cell(&mut self, cell: Option<usize>) {
        self.highlight_cell = cell;
        self.sync_content_presentation();
        if let Some(idx) = self.effective_highlight_cell() {
            self.view.scroll_chunk_into_view(idx);
        }
    }

    pub(crate) fn set_search_state(&mut self, state: Option<TranscriptSearchOverlayState>) {
        self.search_status = state.as_ref().map(|state| state.status.clone());
        self.search_target_chunk = state.as_ref().and_then(|state| state.current_chunk);
        self.search_highlight_cell = state.as_ref().and_then(|state| state.highlight_cell);
        self.search_match_highlight = self.search_status.clone();
        self.sync_content_presentation();
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

    fn sync_content_presentation(&mut self) {
        let mut content = self.content.borrow_mut();
        content.highlight_cell = self.effective_highlight_cell();
        content.search_match = self.search_match_highlight.clone();
    }

    fn effective_highlight_cell(&self) -> Option<usize> {
        self.highlight_cell.or(self.search_highlight_cell)
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
            Paragraph::new(vec![Line::from(search_status.render_text()).dim()]).render(line2, buf);
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
        self.content.borrow().cells.len()
    }
}
