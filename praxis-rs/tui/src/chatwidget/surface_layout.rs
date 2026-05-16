use ratatui::layout::Rect;

use crate::render::Insets;
use crate::render::RectExt as _;

pub(super) const CHAT_SECTION_GAP_ROWS: u16 = 1;
pub(super) const IN_APP_TOAST_ROW_HEIGHT: u16 = 1;

const WORK_PANEL_GAP_COLS: u16 = 1;
const WORK_PANEL_MIN_TERMINAL_WIDTH: u16 = 104;
const WORK_PANEL_MIN_WIDTH: u16 = 30;
const WORK_PANEL_MAX_WIDTH: u16 = 44;
const WORK_PANEL_WIDTH_PERCENT: u16 = 30;
const WORK_PANEL_MIN_AGENT_WIDTH: u16 = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ChatWidgetLayout {
    pub(super) active_outer_area: Option<Rect>,
    pub(super) active_content_area: Option<Rect>,
    pub(super) work_panel_area: Option<Rect>,
    pub(super) toast_area: Option<Rect>,
    pub(super) bottom_outer_area: Rect,
    pub(super) bottom_content_area: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChatSurfaceSplit {
    pub(super) agent_width: u16,
    pub(super) work_panel_width: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChatSurfaceLayoutInput {
    pub(super) area: Rect,
    pub(super) agent_outer_height: u16,
    pub(super) bottom_outer_height: u16,
    pub(super) toast_height: u16,
    pub(super) work_panel_outer_height: u16,
    pub(super) show_work_panel: bool,
}

pub(super) fn chat_surface_split_for_width(width: u16, show_work_panel: bool) -> ChatSurfaceSplit {
    if !show_work_panel || width < WORK_PANEL_MIN_TERMINAL_WIDTH {
        return ChatSurfaceSplit {
            agent_width: width,
            work_panel_width: None,
        };
    }

    let proportional = width.saturating_mul(WORK_PANEL_WIDTH_PERCENT) / 100;
    let panel_width = proportional
        .clamp(WORK_PANEL_MIN_WIDTH, WORK_PANEL_MAX_WIDTH)
        .min(width);
    let reserved = panel_width.saturating_add(WORK_PANEL_GAP_COLS);
    if width <= reserved.saturating_add(WORK_PANEL_MIN_AGENT_WIDTH) {
        return ChatSurfaceSplit {
            agent_width: width,
            work_panel_width: None,
        };
    }

    ChatSurfaceSplit {
        agent_width: width.saturating_sub(reserved),
        work_panel_width: Some(panel_width),
    }
}

pub(super) fn layout_chat_surface(input: ChatSurfaceLayoutInput) -> ChatWidgetLayout {
    let area = input.area;
    if area.is_empty() {
        return ChatWidgetLayout::default();
    }

    let split = chat_surface_split_for_width(area.width, input.show_work_panel);
    let bottom_outer_height = input.bottom_outer_height.min(area.height);
    let toast_height = input
        .toast_height
        .min(area.height.saturating_sub(bottom_outer_height));
    let available_for_top = area
        .height
        .saturating_sub(bottom_outer_height)
        .saturating_sub(toast_height);
    let top_requested = match split.work_panel_width {
        Some(_) => input.agent_outer_height.max(input.work_panel_outer_height),
        None => input.agent_outer_height,
    };
    let top_height = top_requested.min(available_for_top);

    let active_outer_area = if top_height > 0 && split.agent_width > 0 {
        Some(Rect::new(area.x, area.y, split.agent_width, top_height))
    } else {
        None
    };
    let active_content_area = active_outer_area.map(|outer| {
        outer.inset(Insets::tlbr(
            /*top*/ CHAT_SECTION_GAP_ROWS,
            /*left*/ 0,
            /*bottom*/ 0,
            /*right*/ 0,
        ))
    });

    let work_panel_area = split.work_panel_width.and_then(|panel_width| {
        (top_height > 0).then(|| {
            Rect::new(
                area.x
                    .saturating_add(split.agent_width)
                    .saturating_add(WORK_PANEL_GAP_COLS),
                area.y,
                panel_width,
                top_height,
            )
        })
    });

    let toast_area = if toast_height > 0 {
        Some(Rect::new(
            area.x,
            area.y.saturating_add(top_height),
            area.width,
            toast_height,
        ))
    } else {
        None
    };

    let bottom_outer_y = area
        .y
        .saturating_add(top_height)
        .saturating_add(toast_height);
    let bottom_outer_available = area.bottom().saturating_sub(bottom_outer_y);
    let bottom_outer_area = Rect::new(
        area.x,
        bottom_outer_y,
        area.width,
        bottom_outer_height.min(bottom_outer_available),
    );
    let bottom_content_area = bottom_outer_area.inset(Insets::tlbr(
        /*top*/ CHAT_SECTION_GAP_ROWS,
        /*left*/ 0,
        /*bottom*/ 0,
        /*right*/ 0,
    ));

    ChatWidgetLayout {
        active_outer_area,
        active_content_area,
        work_panel_area,
        toast_area,
        bottom_outer_area,
        bottom_content_area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_hides_work_panel_on_narrow_terminals() {
        let split = chat_surface_split_for_width(90, /*show_work_panel*/ true);
        assert_eq!(split.agent_width, 90);
        assert_eq!(split.work_panel_width, None);
    }

    #[test]
    fn split_reserves_a_right_panel_without_starving_chat() {
        let split = chat_surface_split_for_width(140, /*show_work_panel*/ true);
        assert_eq!(split.work_panel_width, Some(42));
        assert_eq!(split.agent_width, 97);
    }

    #[test]
    fn layout_keeps_bottom_pane_full_width_with_work_panel() {
        let layout = layout_chat_surface(ChatSurfaceLayoutInput {
            area: Rect::new(0, 0, 140, 40),
            agent_outer_height: 10,
            bottom_outer_height: 6,
            toast_height: 1,
            work_panel_outer_height: 12,
            show_work_panel: true,
        });

        assert_eq!(layout.active_outer_area, Some(Rect::new(0, 0, 97, 12)));
        assert_eq!(layout.work_panel_area, Some(Rect::new(98, 0, 42, 12)));
        assert_eq!(layout.toast_area, Some(Rect::new(0, 12, 140, 1)));
        assert_eq!(layout.bottom_outer_area, Rect::new(0, 13, 140, 6));
    }
}
