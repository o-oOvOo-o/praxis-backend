use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;

use praxis_protocol::ThreadId;
use ratatui::layout::Rect;

#[derive(Debug)]
pub(crate) struct LaunchStripState {
    pub(crate) rank: u8,
    active_thread_id: Option<ThreadId>,
    rank_by_thread: HashMap<ThreadId, u8>,
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
            active_thread_id: None,
            rank_by_thread: HashMap::new(),
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
        if let Some(thread_id) = self.active_thread_id {
            self.rank_by_thread.insert(thread_id, self.rank);
        }
        self.rank
    }

    pub(crate) fn activate_thread(&mut self, thread_id: ThreadId, default_rank: u8) -> u8 {
        self.active_thread_id = Some(thread_id);
        self.rank = *self.rank_by_thread.entry(thread_id).or_insert(default_rank);
        self.clear_dropdown();
        self.rank
    }
}

fn rect_contains_point(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_selection_is_isolated_per_thread() {
        let thread_a = ThreadId::new();
        let thread_b = ThreadId::new();
        let mut state = LaunchStripState::default();

        assert_eq!(state.activate_thread(thread_a, 0), 0);
        assert_eq!(state.set_rank(1, 2), 1);
        assert_eq!(state.activate_thread(thread_b, 0), 0);
        assert_eq!(state.set_rank(2, 2), 2);
        assert_eq!(state.activate_thread(thread_a, 0), 1);
        assert_eq!(state.activate_thread(thread_b, 0), 2);
    }

    #[test]
    fn first_activation_uses_the_threads_canonical_rank() {
        let mut state = LaunchStripState::default();

        assert_eq!(state.activate_thread(ThreadId::new(), 1), 1);
    }
}
