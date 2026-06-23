use super::*;

pub(super) struct PagerView {
    content: Box<dyn PagerContent>,
    layout_cache: RefCell<PagerLayoutCache>,
    pub(super) scroll_offset: usize,
    title: String,
    last_content_height: Option<usize>,
    last_rendered_height: Option<usize>,
    /// If set, on next render ensure this chunk is visible.
    pending_scroll_chunk: Option<usize>,
    pending_scroll_after_layout: Option<PendingScrollAfterLayout>,
}

impl PagerView {
    pub(super) fn new(
        renderables: Vec<Box<dyn Renderable>>,
        title: String,
        scroll_offset: usize,
    ) -> Self {
        Self::new_with_content(
            Box::new(StaticPagerContent::new(renderables)),
            title,
            scroll_offset,
        )
    }

    pub(super) fn new_with_content(
        content: Box<dyn PagerContent>,
        title: String,
        scroll_offset: usize,
    ) -> Self {
        Self {
            content,
            layout_cache: RefCell::new(PagerLayoutCache::default()),
            scroll_offset,
            title,
            last_content_height: None,
            last_rendered_height: None,
            pending_scroll_chunk: None,
            pending_scroll_after_layout: None,
        }
    }

    pub(super) fn invalidate_layout(&mut self) {
        self.layout_cache.borrow_mut().invalidate();
        self.last_rendered_height = None;
    }

    pub(super) fn content_height(&self, width: u16) -> usize {
        self.with_layout(width, |layout| layout.total_height())
    }

    pub(super) fn with_layout<R>(&self, width: u16, f: impl FnOnce(&PagerLayoutCache) -> R) -> R {
        {
            let mut layout = self.layout_cache.borrow_mut();
            layout.ensure(width, self.content.as_ref());
        }
        let layout = self.layout_cache.borrow();
        f(&layout)
    }

    pub(super) fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        self.render_header(area, buf);
        let content_area = self.content_area(area);
        self.update_last_content_height(content_area.height);

        if self.should_render_bottom_fast(content_area.width) {
            self.last_rendered_height = None;
            self.render_bottom_content(content_area, buf);
            self.render_bottom_bar(area, content_area, buf, content_area.height as usize);
            return;
        }

        let content_height = self.content_height(content_area.width);
        self.last_rendered_height = Some(content_height);
        self.apply_pending_scroll_after_layout(content_height, content_area.height);
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

    pub(super) fn should_render_bottom_fast(&self, width: u16) -> bool {
        self.scroll_offset == usize::MAX
            && self.pending_scroll_after_layout.is_none()
            && !self.has_current_layout(width)
    }

    pub(super) fn has_current_layout(&self, width: u16) -> bool {
        self.layout_cache
            .borrow()
            .is_current(width, self.content.len())
    }

    pub(super) fn render_header(&self, area: Rect, buf: &mut Buffer) {
        Span::from("/ ".repeat(area.width as usize / 2))
            .dim()
            .render(area, buf);
        let header = format!("/ {}", self.title);
        header.dim().render(area, buf);
    }

    pub(super) fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let mut drawn_bottom = area.y;

        self.with_layout(area.width, |layout| {
            let viewport_top = self.scroll_offset;
            let viewport_bottom = viewport_top.saturating_add(area.height as usize);
            let Some(start_idx) = layout.first_visible_index(viewport_top) else {
                return;
            };

            for idx in start_idx..self.content.len() {
                let Some((chunk_top, chunk_bottom)) = layout.chunk_bounds(idx) else {
                    break;
                };
                if chunk_top >= viewport_bottom {
                    break;
                }
                if chunk_bottom <= viewport_top || chunk_top == chunk_bottom {
                    continue;
                }

                let visible_top = chunk_top.max(viewport_top);
                let visible_bottom = chunk_bottom.min(viewport_bottom);
                if visible_top >= visible_bottom {
                    continue;
                }

                let y_offset = visible_top.saturating_sub(viewport_top) as u16;
                let draw_height = visible_bottom.saturating_sub(visible_top) as u16;
                let draw_area = Rect::new(
                    area.x,
                    area.y.saturating_add(y_offset),
                    area.width,
                    draw_height,
                );
                if visible_top > chunk_top {
                    let scroll_offset = visible_top.saturating_sub(chunk_top) as u16;
                    let drawn = render_offset_content(
                        draw_area,
                        buf,
                        self.content.as_ref(),
                        idx,
                        scroll_offset,
                    );
                    drawn_bottom = drawn_bottom.max(draw_area.y.saturating_add(drawn));
                } else {
                    self.content.render(idx, draw_area, buf);
                    drawn_bottom = drawn_bottom.max(draw_area.y.saturating_add(draw_area.height));
                }
            }
        });

        for y in drawn_bottom..area.bottom() {
            draw_empty_pager_line(area, buf, y);
        }
    }

    pub(super) fn render_bottom_content(&self, area: Rect, buf: &mut Buffer) {
        let mut remaining = area.height as usize;
        let mut visible_chunks = Vec::new();

        for idx in (0..self.content.len()).rev() {
            let height = self.content.desired_height(idx, area.width) as usize;
            if height == 0 {
                continue;
            }
            let visible_height = height.min(remaining);
            let scroll_offset = height.saturating_sub(visible_height);
            visible_chunks.push((idx, visible_height as u16, scroll_offset as u16));
            remaining = remaining.saturating_sub(visible_height);
            if remaining == 0 {
                break;
            }
        }

        visible_chunks.reverse();
        let mut y = area.y;
        for (idx, height, scroll_offset) in visible_chunks {
            if height == 0 {
                continue;
            }
            let draw_area = Rect::new(area.x, y, area.width, height);
            if scroll_offset > 0 {
                self.content
                    .render_window(idx, draw_area, buf, scroll_offset);
            } else {
                self.content.render(idx, draw_area, buf);
            }
            y = y.saturating_add(height);
        }

        for y in y..area.bottom() {
            draw_empty_pager_line(area, buf, y);
        }
    }

    pub(super) fn render_bottom_bar(
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
            .render(sep_rect, buf);
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
            .render(Rect::new(pct_x, sep_rect.y, pct_w, 1), buf);
    }

    pub(super) fn handle_key_event(
        &mut self,
        tui: &mut tui::Tui,
        key_event: KeyEvent,
    ) -> Result<()> {
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

    /// Returns the height of one page in content rows.
    ///
    /// Prefers the last rendered content height (excluding header/footer chrome);
    /// if no render has occurred yet, falls back to the content area height
    /// computed from the given viewport.
    pub(super) fn page_height(&self, viewport_area: Rect) -> usize {
        self.last_content_height
            .unwrap_or_else(|| self.content_area(viewport_area).height as usize)
    }

    pub(super) fn update_last_content_height(&mut self, height: u16) {
        self.last_content_height = Some(height as usize);
    }

    pub(super) fn content_area(&self, area: Rect) -> Rect {
        let mut area = area;
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(2);
        area
    }

    pub(super) fn max_scroll_for_known_layout(&self) -> Option<usize> {
        let total_height = self.last_rendered_height?;
        let content_height = self.last_content_height?;
        Some(total_height.saturating_sub(content_height))
    }

    pub(super) fn normalized_scroll_offset(&self) -> usize {
        self.max_scroll_for_known_layout()
            .map_or(self.scroll_offset, |max_scroll| {
                self.scroll_offset.min(max_scroll)
            })
    }

    pub(super) fn scroll_up(&mut self, amount: usize) {
        if self.needs_layout_before_relative_scroll() {
            self.queue_scroll_up_after_layout(amount);
            return;
        }
        self.scroll_offset = self.normalized_scroll_offset().saturating_sub(amount);
    }

    pub(super) fn scroll_down(&mut self, amount: usize) {
        let next = self.normalized_scroll_offset().saturating_add(amount);
        self.scroll_offset = self
            .max_scroll_for_known_layout()
            .map_or(next, |max_scroll| next.min(max_scroll));
    }

    pub(super) fn needs_layout_before_relative_scroll(&self) -> bool {
        self.max_scroll_for_known_layout().is_none() && self.scroll_offset > usize::MAX / 2
    }

    pub(super) fn queue_scroll_up_after_layout(&mut self, amount: usize) {
        let amount = match self.pending_scroll_after_layout.take() {
            Some(PendingScrollAfterLayout::Up(existing)) => existing.saturating_add(amount),
            None => amount,
        };
        self.pending_scroll_after_layout = Some(PendingScrollAfterLayout::Up(amount));
        self.scroll_offset = usize::MAX.saturating_sub(1);
    }

    pub(super) fn apply_pending_scroll_after_layout(
        &mut self,
        total_height: usize,
        viewport_height: u16,
    ) {
        let max_scroll = total_height.saturating_sub(viewport_height as usize);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
        let Some(pending) = self.pending_scroll_after_layout.take() else {
            return;
        };
        match pending {
            PendingScrollAfterLayout::Up(amount) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(amount);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingScrollAfterLayout {
    Up(usize),
}

pub(super) fn draw_empty_pager_line(area: Rect, buf: &mut Buffer, y: u16) {
    if area.width == 0 {
        return;
    }
    buf[(area.x, y)] = Cell::from('~');
    for x in area.x + 1..area.right() {
        buf[(x, y)] = Cell::from(' ');
    }
}

impl PagerView {
    pub(super) fn is_scrolled_to_bottom(&self) -> bool {
        if self.scroll_offset == usize::MAX {
            return true;
        }
        let Some(height) = self.last_content_height else {
            return false;
        };
        if self.content.len() == 0 {
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
    pub(super) fn scroll_chunk_into_view(&mut self, chunk_index: usize) {
        self.pending_scroll_chunk = Some(chunk_index);
    }

    pub(super) fn ensure_chunk_visible(&mut self, idx: usize, area: Rect) {
        if area.height == 0 || idx >= self.content.len() {
            return;
        }
        let Some((first, last)) = self.with_layout(area.width, |layout| layout.chunk_bounds(idx))
        else {
            return;
        };
        let viewport_top = self.scroll_offset;
        let viewport_bottom = viewport_top.saturating_add(area.height as usize);
        if first < viewport_top {
            self.scroll_offset = first;
        } else if last > viewport_bottom {
            self.scroll_offset = last.saturating_sub(area.height as usize);
        }
    }
}

pub(super) trait PagerContent {
    fn len(&self) -> usize;
    fn desired_height(&self, idx: usize, width: u16) -> u16;
    fn render(&self, idx: usize, area: Rect, buf: &mut Buffer);
    fn render_window(
        &self,
        idx: usize,
        area: Rect,
        buf: &mut Buffer,
        scroll_offset: u16,
    );
}

struct StaticPagerContent {
    renderables: Vec<Box<dyn Renderable>>,
}

impl StaticPagerContent {
    pub(super) fn new(renderables: Vec<Box<dyn Renderable>>) -> Self {
        Self { renderables }
    }
}

impl PagerContent for StaticPagerContent {
    fn len(&self) -> usize {
        self.renderables.len()
    }

    fn desired_height(&self, idx: usize, width: u16) -> u16 {
        self.renderables
            .get(idx)
            .map(|renderable| renderable.desired_height(width))
            .unwrap_or(0)
    }

    fn render(&self, idx: usize, area: Rect, buf: &mut Buffer) {
        if let Some(renderable) = self.renderables.get(idx) {
            renderable.render(area, buf);
        }
    }

    fn render_window(
        &self,
        idx: usize,
        area: Rect,
        buf: &mut Buffer,
        scroll_offset: u16,
    ) {
        if let Some(renderable) = self.renderables.get(idx) {
            renderable.render_window(area, buf, scroll_offset);
        }
    }
}

#[derive(Default)]
struct PagerLayoutCache {
    width: Option<u16>,
    renderable_count: usize,
    heights: Vec<usize>,
    prefix_offsets: Vec<usize>,
}

impl PagerLayoutCache {
    pub(super) fn invalidate(&mut self) {
        self.width = None;
        self.renderable_count = 0;
        self.heights.clear();
        self.prefix_offsets.clear();
    }

    pub(super) fn is_current(&self, width: u16, content_len: usize) -> bool {
        self.width == Some(width) && self.renderable_count == content_len
    }

    pub(super) fn ensure(&mut self, width: u16, content: &dyn PagerContent) {
        if self.is_current(width, content.len()) {
            return;
        }

        self.width = Some(width);
        self.renderable_count = content.len();
        self.heights.clear();
        self.prefix_offsets.clear();
        self.heights.reserve(content.len());
        self.prefix_offsets.reserve(content.len().saturating_add(1));
        self.prefix_offsets.push(0);

        let mut total = 0usize;
        for idx in 0..content.len() {
            let height = content.desired_height(idx, width) as usize;
            self.heights.push(height);
            total = total.saturating_add(height);
            self.prefix_offsets.push(total);
        }
    }

    pub(super) fn total_height(&self) -> usize {
        self.prefix_offsets.last().copied().unwrap_or(0)
    }

    pub(super) fn first_visible_index(&self, viewport_top: usize) -> Option<usize> {
        if self.heights.is_empty() || viewport_top >= self.total_height() {
            return None;
        }

        let mut low = 1usize;
        let mut high = self.prefix_offsets.len();
        while low < high {
            let mid = low + (high - low) / 2;
            if self.prefix_offsets[mid] <= viewport_top {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Some(low.saturating_sub(1))
    }

    pub(super) fn chunk_bounds(&self, idx: usize) -> Option<(usize, usize)> {
        let top = *self.prefix_offsets.get(idx)?;
        let bottom = *self.prefix_offsets.get(idx + 1)?;
        Some((top, bottom))
    }
}

/// A renderable that caches its desired height.
pub(super) fn render_offset_content(
    area: Rect,
    buf: &mut Buffer,
    content: &dyn PagerContent,
    idx: usize,
    scroll_offset: u16,
) -> u16 {
    let height = content.desired_height(idx, area.width);
    let copy_height = area.height.min(height.saturating_sub(scroll_offset));
    if copy_height > 0 {
        let draw_area = Rect::new(area.x, area.y, area.width, copy_height);
        content.render_window(idx, draw_area, buf, scroll_offset);
    }

    copy_height
}
