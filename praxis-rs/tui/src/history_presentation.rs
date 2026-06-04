//! Shared global fold/expand presentation state for transcript cells.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct PatchCellId(u64);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum FoldCategory {
    Reasoning,
    ToolOutput,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HistoryPresentationDefaults {
    reasoning_expanded: bool,
    tool_output_expanded: bool,
}

#[derive(Default)]
struct RegistryState {
    defaults: HistoryPresentationDefaults,
    expanded_diff_cells: HashSet<PatchCellId>,
}

static REGISTRY: OnceLock<Mutex<RegistryState>> = OnceLock::new();
static PRESENTATION_REVISION: AtomicU64 = AtomicU64::new(0);
static NEXT_PATCH_CELL_ID: AtomicU64 = AtomicU64::new(1);

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

pub(crate) fn next_patch_cell_id() -> PatchCellId {
    PatchCellId(NEXT_PATCH_CELL_ID.fetch_add(1, Ordering::Relaxed))
}

pub(crate) fn is_diff_cell_expanded(id: PatchCellId) -> bool {
    let guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    guard.expanded_diff_cells.contains(&id)
}

pub(crate) fn toggle_diff_cells(ids: &[PatchCellId]) -> bool {
    let mut unique_ids = Vec::new();
    for id in ids.iter().copied() {
        if !unique_ids.contains(&id) {
            unique_ids.push(id);
        }
    }
    if unique_ids.is_empty() {
        return false;
    }

    let mut guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    let should_expand = unique_ids
        .iter()
        .any(|id| !guard.expanded_diff_cells.contains(id));
    if should_expand {
        guard.expanded_diff_cells.extend(unique_ids);
    } else {
        for id in unique_ids {
            guard.expanded_diff_cells.remove(&id);
        }
    }
    bump_revision();
    true
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
