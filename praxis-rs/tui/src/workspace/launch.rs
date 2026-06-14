use std::cell::Cell;
use std::cell::RefCell;

use ratatui::layout::Rect;

#[derive(Debug)]
pub(crate) struct LaunchStripState {
    pub(crate) rank: u8,
    pub(crate) dropdown: Option<LaunchStripDropdown>,
    pub(crate) model_area: Cell<Option<Rect>>,
    pub(crate) reasoning_area: Cell<Option<Rect>>,
    pub(crate) rank_area: Cell<Option<Rect>>,
    pub(crate) permissions_area: Cell<Option<Rect>>,
    pub(crate) dropdown_targets: RefCell<Vec<LaunchStripDropdownMouseTarget>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaunchStripDropdown {
    Model,
    Reasoning,
    Rank,
    Permissions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaunchStripMouseAction {
    ToggleModelDropdown,
    ToggleReasoningDropdown,
    ToggleRankDropdown,
    TogglePermissionsDropdown,
    SelectModel(usize),
    SelectReasoning(usize),
    SelectRank(u8),
    SelectPermission(usize),
    DismissDropdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaunchStripDropdownMouseTarget {
    pub(crate) area: Rect,
    pub(crate) action: LaunchStripMouseAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaunchStripDropdownItem {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) is_current: bool,
    pub(crate) is_disabled: bool,
}

impl Default for LaunchStripState {
    fn default() -> Self {
        Self {
            rank: 0,
            dropdown: None,
            model_area: Cell::new(None),
            reasoning_area: Cell::new(None),
            rank_area: Cell::new(None),
            permissions_area: Cell::new(None),
            dropdown_targets: RefCell::new(Vec::new()),
        }
    }
}

impl LaunchStripState {
    pub(crate) fn mouse_action(&self, column: u16, row: u16) -> Option<LaunchStripMouseAction> {
        if let Some(target) = self
            .dropdown_targets
            .borrow()
            .iter()
            .find(|target| rect_contains_point(target.area, column, row))
        {
            return Some(target.action);
        }

        if self
            .model_area
            .get()
            .is_some_and(|area| rect_contains_point(area, column, row))
        {
            return Some(LaunchStripMouseAction::ToggleModelDropdown);
        }
        if self
            .reasoning_area
            .get()
            .is_some_and(|area| rect_contains_point(area, column, row))
        {
            return Some(LaunchStripMouseAction::ToggleReasoningDropdown);
        }
        if self
            .rank_area
            .get()
            .is_some_and(|area| rect_contains_point(area, column, row))
        {
            return Some(LaunchStripMouseAction::ToggleRankDropdown);
        }
        if self
            .permissions_area
            .get()
            .is_some_and(|area| rect_contains_point(area, column, row))
        {
            return Some(LaunchStripMouseAction::TogglePermissionsDropdown);
        }

        self.dropdown
            .is_some()
            .then_some(LaunchStripMouseAction::DismissDropdown)
    }

    pub(crate) fn clear_hit_areas(&self) {
        self.model_area.set(None);
        self.reasoning_area.set(None);
        self.rank_area.set(None);
        self.permissions_area.set(None);
        self.dropdown_targets.borrow_mut().clear();
    }

    pub(crate) fn clear_dropdown(&mut self) {
        self.dropdown = None;
    }

    pub(crate) fn toggle_dropdown(&mut self, dropdown: LaunchStripDropdown) {
        self.dropdown = if self.dropdown == Some(dropdown) {
            None
        } else {
            Some(dropdown)
        };
    }

    pub(crate) fn set_rank(&mut self, rank: u8, max_rank: u8) -> u8 {
        self.rank = rank.min(max_rank);
        self.rank
    }
}

pub(crate) fn rect_contains_point(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}
