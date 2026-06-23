use std::cell::Ref;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use super::ActiveCellRenderCache;
use super::ActiveCellRenderCacheKey;
use super::ChatWidget;
use super::WorkspaceActiveTailCache;
use super::surface_layout::CHAT_SECTION_GAP_ROWS;
use super::surface_layout::ChatWidgetLayout;
use crate::history_cell;
use crate::history_cell::ChatLane;
use crate::history_cell::HistoryCell;
use crate::workspace::WorkspaceTranscriptRequest;
use crate::workspace::WorkspaceTranscriptTail;
use crate::workspace::WorkspaceTranscriptViewport;
use crate::workspace::render_workspace_transcript_viewport as render_workspace_transcript_viewport_rows;
use crate::workspace::workspace_transcript_lane_width;
use crate::workspace::wrap_workspace_transcript_lines;

impl ChatWidget {
    pub(super) fn render_workspace_transcript_viewport(
        &self,
        viewport: &WorkspaceTranscriptViewport,
        buf: &mut Buffer,
    ) {
        render_workspace_transcript_viewport_rows(viewport, buf);
    }

    pub(super) fn workspace_transcript_viewport(
        &self,
        layout: ChatWidgetLayout,
        transcript_cells: &[Arc<dyn HistoryCell>],
        scroll_from_bottom: usize,
    ) -> Option<WorkspaceTranscriptViewport> {
        let content_area = layout.active_content_area?;
        if content_area.is_empty() {
            return None;
        }

        let request =
            self.workspace_transcript_request(content_area, transcript_cells, scroll_from_bottom);
        Some(
            self.workspace_transcript_cache
                .borrow_mut()
                .viewport(request),
        )
    }

    pub(super) fn workspace_transcript_scroll_limit(
        &self,
        content_area: Rect,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> usize {
        let request = self.workspace_transcript_request(
            content_area,
            transcript_cells,
            /*scroll_from_bottom*/ 0,
        );
        self.workspace_transcript_cache
            .borrow_mut()
            .scroll_limit(request)
    }

    fn workspace_transcript_request<'a>(
        &self,
        content_area: Rect,
        transcript_cells: &'a [Arc<dyn HistoryCell>],
        scroll_from_bottom: usize,
    ) -> WorkspaceTranscriptRequest<'a> {
        WorkspaceTranscriptRequest {
            content_area,
            transcript_cells,
            scroll_from_bottom,
            theme: self.workspace_theme(),
            theme_kind: self.workspace_theme_kind(),
            presentation_revision: history_cell::history_presentation_revision(),
            active_tail: self.workspace_transcript_tail(content_area.width),
        }
    }

    fn workspace_transcript_tail(&self, width: u16) -> Option<WorkspaceTranscriptTail> {
        let cache = self.workspace_active_tail_cache(width)?;
        Some(WorkspaceTranscriptTail {
            lane: cache.lane,
            lines: cache.lines.clone(),
            patch_cell_id: self
                .active_cell
                .as_ref()
                .and_then(|cell| cell.patch_cell_id()),
        })
    }

    fn workspace_active_tail_cache(&self, width: u16) -> Option<Ref<'_, WorkspaceActiveTailCache>> {
        let Some(key) = self.active_cell_render_cache_key(width) else {
            self.workspace_active_tail_cache.borrow_mut().take();
            return None;
        };
        let needs_refresh = self
            .workspace_active_tail_cache
            .borrow()
            .as_ref()
            .is_none_or(|cache| cache.key != key);
        if needs_refresh {
            let lane = self
                .active_cell
                .as_ref()
                .map(|cell| cell.chat_lane())
                .unwrap_or(ChatLane::Assistant);
            let lane_width = workspace_transcript_lane_width(width, lane);
            let lines = {
                let cache = self.active_cell_render_cache(lane_width)?;
                wrap_workspace_transcript_lines(cache.lines.clone(), lane_width)
            };
            *self.workspace_active_tail_cache.borrow_mut() =
                Some(WorkspaceActiveTailCache { key, lane, lines });
        }

        Some(Ref::map(
            self.workspace_active_tail_cache.borrow(),
            |cache| {
                cache
                    .as_ref()
                    .expect("workspace active tail cache should be populated")
            },
        ))
    }

    fn active_cell_render_cache_key(&self, width: u16) -> Option<ActiveCellRenderCacheKey> {
        let cell = self.active_cell.as_ref()?;
        Some(ActiveCellRenderCacheKey {
            width,
            revision: self.active_cell_revision,
            animation_tick: cell.transcript_animation_tick(),
            presentation_revision: history_cell::history_presentation_revision(),
        })
    }

    pub(super) fn active_cell_render_cache(
        &self,
        width: u16,
    ) -> Option<Ref<'_, ActiveCellRenderCache>> {
        let key = self.active_cell_render_cache_key(width)?;
        let needs_refresh = self
            .active_cell_render_cache
            .borrow()
            .as_ref()
            .is_none_or(|cache| cache.key != key);
        if needs_refresh {
            let cell = self.active_cell.as_ref()?;
            *self.active_cell_render_cache.borrow_mut() =
                Some(Self::build_active_cell_render_cache(cell.as_ref(), key));
        }

        Some(Ref::map(self.active_cell_render_cache.borrow(), |cache| {
            cache
                .as_ref()
                .expect("active cell render cache should be populated")
        }))
    }

    fn build_active_cell_render_cache(
        cell: &dyn HistoryCell,
        key: ActiveCellRenderCacheKey,
    ) -> ActiveCellRenderCache {
        let lines = cell.display_lines(key.width);
        let desired_height = Paragraph::new(Text::from(lines.clone()))
            .wrap(Wrap { trim: false })
            .line_count(key.width)
            .try_into()
            .unwrap_or(0);
        let mouse_targets = cell.mouse_targets(key.width);
        ActiveCellRenderCache {
            key,
            lines,
            desired_height,
            mouse_targets,
        }
    }

    pub(super) fn active_cell_total_height(&self, width: u16) -> u16 {
        self.active_cell_render_cache(width)
            .map(|cache| cache.desired_height.saturating_add(CHAT_SECTION_GAP_ROWS))
            .unwrap_or(0)
    }
}
