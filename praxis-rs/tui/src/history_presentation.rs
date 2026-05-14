//! Shared fold/expand presentation state for transcript cells.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct HistoryPresentationKey(u64);

impl HistoryPresentationKey {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) fn from_hash(namespace: &str, value: &impl Hash) -> Self {
        stable_key(namespace, value)
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum FoldCategory {
    Reasoning,
    ToolOutput,
    Diff,
    RawToolInput,
    QueueSummary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HistoryPresentationDefaults {
    pub(crate) reasoning_expanded: bool,
    pub(crate) tool_output_expanded: bool,
    pub(crate) diff_expanded: bool,
    pub(crate) raw_tool_input_expanded: bool,
    pub(crate) queue_summary_expanded: bool,
}

pub(crate) type HistoryPresentationState = HistoryPresentationDefaults;

#[derive(Default)]
struct RegistryState {
    defaults: HistoryPresentationDefaults,
    overrides: HashMap<(FoldCategory, HistoryPresentationKey), bool>,
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
        FoldCategory::RawToolInput => defaults.raw_tool_input_expanded,
        FoldCategory::QueueSummary => defaults.queue_summary_expanded,
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
        FoldCategory::RawToolInput => defaults.raw_tool_input_expanded = expanded,
        FoldCategory::QueueSummary => defaults.queue_summary_expanded = expanded,
    }
}

pub(crate) fn presentation_revision() -> u64 {
    PRESENTATION_REVISION.load(Ordering::Relaxed)
}

pub(crate) fn defaults() -> HistoryPresentationDefaults {
    registry()
        .lock()
        .expect("history presentation mutex poisoned")
        .defaults
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

pub(crate) fn set_expanded(category: FoldCategory, key: HistoryPresentationKey, expanded: bool) {
    let mut guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    guard.overrides.insert((category, key), expanded);
    bump_revision();
}

pub(crate) fn toggle_expanded(category: FoldCategory, key: HistoryPresentationKey) -> bool {
    let mut guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    let current = guard
        .overrides
        .get(&(category, key))
        .copied()
        .unwrap_or_else(|| default_flag(guard.defaults, category));
    let next = !current;
    guard.overrides.insert((category, key), next);
    bump_revision();
    next
}

pub(crate) fn is_expanded(key: Option<HistoryPresentationKey>, category: FoldCategory) -> bool {
    let guard = registry()
        .lock()
        .expect("history presentation mutex poisoned");
    key.and_then(|key| guard.overrides.get(&(category, key)).copied())
        .unwrap_or_else(|| default_flag(guard.defaults, category))
}

pub(crate) fn stable_key<T: Hash>(category: &str, value: &T) -> HistoryPresentationKey {
    let mut hasher = DefaultHasher::new();
    category.hash(&mut hasher);
    value.hash(&mut hasher);
    HistoryPresentationKey::new(hasher.finish())
}

pub(crate) fn default_expanded(category: FoldCategory) -> bool {
    default_flag(defaults(), category)
}

pub(crate) fn history_presentation_revision() -> u64 {
    presentation_revision()
}

pub(crate) fn history_presentation_state() -> HistoryPresentationState {
    defaults()
}

pub(crate) fn next_history_presentation_key() -> HistoryPresentationKey {
    HistoryPresentationKey::new(PRESENTATION_REVISION.fetch_add(1, Ordering::Relaxed) + 1)
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
    fn override_beats_default() {
        let key = stable_key("reasoning", &"cell-a");
        set_expanded(FoldCategory::Reasoning, key, true);

        assert!(is_expanded(Some(key), FoldCategory::Reasoning));
    }

    #[test]
    fn toggling_default_flips_visible_state_without_override() {
        let key = stable_key("diff", &"cell-b");
        let before = is_expanded(Some(key), FoldCategory::Diff);
        let after = toggle_default(FoldCategory::Diff);

        assert_eq!(after, !before);
        assert_eq!(is_expanded(Some(key), FoldCategory::Diff), after);
    }
}
