//! Shared global fold/expand presentation state for transcript cells.

use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum FoldCategory {
    Reasoning,
    ToolOutput,
    Diff,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HistoryPresentationDefaults {
    reasoning_expanded: bool,
    tool_output_expanded: bool,
    diff_expanded: bool,
}

#[derive(Default)]
struct RegistryState {
    defaults: HistoryPresentationDefaults,
}

static REGISTRY: OnceLock<Mutex<RegistryState>> = OnceLock::new();
static PRESENTATION_REVISION: AtomicU64 = AtomicU64::new(0);

fn registry() -> &'static Mutex<RegistryState> {
    REGISTRY.get_or_init(|| Mutex::new(RegistryState::default()))
}

fn bump_revision() {
    PRESENTATION_REVISION.fetch_add(1, Ordering::Relaxed);
}

fn default_flag(defaults: HistoryPresentationDefaults, category: FoldCategory) -> bool {
    match category {
        FoldCategory::Reasoning => defaults.reasoning_expanded,
        FoldCategory::ToolOutput => defaults.tool_output_expanded,
        FoldCategory::Diff => defaults.diff_expanded,
    }
}

fn set_default_flag(
    defaults: &mut HistoryPresentationDefaults,
    category: FoldCategory,
    expanded: bool,
) {
    match category {
        FoldCategory::Reasoning => defaults.reasoning_expanded = expanded,
        FoldCategory::ToolOutput => defaults.tool_output_expanded = expanded,
        FoldCategory::Diff => defaults.diff_expanded = expanded,
    }
}

pub(crate) fn toggle_default(category: FoldCategory) -> bool {
    let mut guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    let next = !default_flag(guard.defaults, category);
    set_default_flag(&mut guard.defaults, category, next);
    bump_revision();
    next
}

pub(crate) fn is_expanded(category: FoldCategory) -> bool {
    let guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    default_flag(guard.defaults, category)
}

pub(crate) fn history_presentation_revision() -> u64 {
    PRESENTATION_REVISION.load(Ordering::Relaxed)
}

pub(crate) fn toggle_reasoning_expanded() -> bool {
    toggle_default(FoldCategory::Reasoning)
}

pub(crate) fn toggle_tool_output_expanded() -> bool {
    toggle_default(FoldCategory::ToolOutput)
}

pub(crate) fn toggle_diff_expanded() -> bool {
    toggle_default(FoldCategory::Diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn toggling_default_flips_visible_state() {
        let before = is_expanded(FoldCategory::Diff);
        let after = toggle_default(FoldCategory::Diff);

        assert_eq!(after, !before);
        assert_eq!(is_expanded(FoldCategory::Diff), after);
    }
}
