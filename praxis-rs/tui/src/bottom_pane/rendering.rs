use super::BottomPane;
use crate::render::renderable::FlexRenderable;
use crate::render::renderable::Renderable;
use crate::render::renderable::RenderableItem;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl BottomPane {
    fn as_renderable(&'_ self) -> RenderableItem<'_> {
        if let Some(view) = self.active_view() {
            RenderableItem::Borrowed(view)
        } else {
            let mut flex = FlexRenderable::new();
            if let Some(status) = &self.status {
                flex.push(/*flex*/ 0, RenderableItem::Borrowed(status));
            }
            // Avoid double-surfacing the same summary and avoid adding an extra
            // row while the status line is already visible.
            if self.status.is_none() && !self.unified_exec_footer.is_empty() {
                flex.push(
                    /*flex*/ 0,
                    RenderableItem::Borrowed(&self.unified_exec_footer),
                );
            }
            let has_pending_thread_approvals = !self.pending_thread_approvals.is_empty();
            let has_pending_input = !self.pending_input_preview.queued_messages.is_empty()
                || !self.pending_input_preview.pending_steers.is_empty()
                || !self.pending_input_preview.rejected_steers.is_empty();
            let has_status_or_footer =
                self.status.is_some() || !self.unified_exec_footer.is_empty();
            let has_inline_previews = has_pending_thread_approvals || has_pending_input;
            if has_inline_previews && has_status_or_footer {
                flex.push(/*flex*/ 0, RenderableItem::Owned("".into()));
            }
            flex.push(
                /*flex*/ 1,
                RenderableItem::Borrowed(&self.pending_thread_approvals),
            );
            if has_pending_thread_approvals && has_pending_input {
                flex.push(/*flex*/ 0, RenderableItem::Owned("".into()));
            }
            flex.push(
                /*flex*/ 1,
                RenderableItem::Borrowed(&self.pending_input_preview),
            );
            if !has_inline_previews && has_status_or_footer {
                flex.push(/*flex*/ 0, RenderableItem::Owned("".into()));
            }
            let mut flex2 = FlexRenderable::new();
            flex2.push(/*flex*/ 1, RenderableItem::Owned(flex.into()));
            flex2.push(/*flex*/ 0, RenderableItem::Borrowed(&self.composer));
            RenderableItem::Owned(Box::new(flex2))
        }
    }
}

impl Renderable for BottomPane {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.as_renderable().render(area, buf);
    }
    fn desired_height(&self, width: u16) -> u16 {
        self.as_renderable().desired_height(width)
    }
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.as_renderable().cursor_pos(area)
    }
}
