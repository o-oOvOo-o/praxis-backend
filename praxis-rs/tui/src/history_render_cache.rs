//! Lightweight render cache for transcript/history cells.

use std::collections::HashMap;
use std::collections::VecDeque;

use ratatui::text::Line;

use crate::history_cell::HistoryCellMouseTarget;
use crate::history_presentation::HistoryPresentationKey;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct HistoryRenderCacheKey {
    pub(crate) presentation_key: Option<HistoryPresentationKey>,
    pub(crate) width: u16,
    pub(crate) revision: u64,
    pub(crate) animation_tick: Option<u64>,
    pub(crate) presentation_revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedHistoryRender {
    pub(crate) key: HistoryRenderCacheKey,
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) desired_height: u16,
    pub(crate) mouse_targets: Vec<HistoryCellMouseTarget>,
}

#[derive(Clone, Debug)]
pub(crate) struct HistoryRenderCache {
    entries: HashMap<HistoryRenderCacheKey, CachedHistoryRender>,
    order: VecDeque<HistoryRenderCacheKey>,
    max_entries: usize,
}

impl Default for HistoryRenderCache {
    fn default() -> Self {
        Self::new(16)
    }
}

impl HistoryRenderCache {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
        }
    }

    pub(crate) fn get(&mut self, key: &HistoryRenderCacheKey) -> Option<&CachedHistoryRender> {
        if !self.entries.contains_key(key) {
            return None;
        }
        self.touch(*key);
        self.entries.get(key)
    }

    pub(crate) fn get_or_insert_with<F>(
        &mut self,
        key: HistoryRenderCacheKey,
        build: F,
    ) -> &CachedHistoryRender
    where
        F: FnOnce(HistoryRenderCacheKey) -> CachedHistoryRender,
    {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.entries.entry(key) {
            entry.insert(build(key));
        } else {
            self.touch(key);
            return self
                .entries
                .get(&key)
                .expect("render cache entry must exist after lookup");
        }
        self.touch(key);
        self.evict_if_needed();

        self.entries
            .get(&key)
            .expect("render cache entry must exist after insertion")
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub(crate) fn invalidate_presentation_key(
        &mut self,
        presentation_key: Option<HistoryPresentationKey>,
    ) {
        self.order
            .retain(|key| key.presentation_key != presentation_key);
        self.entries
            .retain(|key, _| key.presentation_key != presentation_key);
    }

    fn touch(&mut self, key: HistoryRenderCacheKey) {
        if let Some(idx) = self.order.iter().position(|existing| *existing == key) {
            self.order.remove(idx);
        }
        self.order.push_back(key);
    }

    fn evict_if_needed(&mut self) {
        while self.order.len() > self.max_entries {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&evicted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    fn cache_entry(key: HistoryRenderCacheKey) -> CachedHistoryRender {
        CachedHistoryRender {
            key,
            lines: vec![Line::from("cached")],
            desired_height: 1,
            mouse_targets: Vec::new(),
        }
    }

    #[test]
    fn width_changes_create_distinct_entries() {
        let mut cache = HistoryRenderCache::new(4);
        let key_a = HistoryRenderCacheKey {
            presentation_key: None,
            width: 80,
            revision: 1,
            animation_tick: None,
            presentation_revision: 0,
        };
        let key_b = HistoryRenderCacheKey {
            width: 120,
            ..key_a
        };
        cache.get_or_insert_with(key_a, cache_entry);
        cache.get_or_insert_with(key_b, cache_entry);

        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn invalidating_presentation_key_drops_matching_entries() {
        let mut cache = HistoryRenderCache::new(4);
        let presentation_key = Some(HistoryPresentationKey::new(7));
        let key = HistoryRenderCacheKey {
            presentation_key,
            width: 80,
            revision: 1,
            animation_tick: None,
            presentation_revision: 0,
        };
        cache.get_or_insert_with(key, cache_entry);
        cache.invalidate_presentation_key(presentation_key);

        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn cache_hit_refreshes_lru_order() {
        let mut cache = HistoryRenderCache::new(2);
        let key_a = HistoryRenderCacheKey {
            presentation_key: None,
            width: 80,
            revision: 1,
            animation_tick: None,
            presentation_revision: 0,
        };
        let key_b = HistoryRenderCacheKey {
            revision: 2,
            ..key_a
        };
        let key_c = HistoryRenderCacheKey {
            revision: 3,
            ..key_a
        };

        cache.get_or_insert_with(key_a, cache_entry);
        cache.get_or_insert_with(key_b, cache_entry);
        assert!(
            cache.get(&key_a).is_some(),
            "expected first entry to be cached"
        );

        cache.get_or_insert_with(key_c, cache_entry);

        assert!(
            cache.get(&key_a).is_some(),
            "recently used entry should stay cached"
        );
        assert!(
            cache.get(&key_b).is_none(),
            "least recently used entry should be evicted"
        );
    }
}
