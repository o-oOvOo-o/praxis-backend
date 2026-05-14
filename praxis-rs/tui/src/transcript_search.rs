//! Shared transcript-search state and navigation helpers.
//!
//! The TUI transcript overlay can render committed history plus an optional in-flight live tail.
//! This module keeps the search model independent from any particular widget so the app layer can
//! wire keys/UI later while `ChatWidget` and the overlay reuse the same match/index state.
#![cfg_attr(not(test), allow(dead_code))]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSearchTarget {
    /// Renderable chunk index in the transcript overlay.
    pub(crate) chunk_index: usize,
    /// Matching committed cell, if the hit belongs to committed transcript history.
    pub(crate) cell_index: Option<usize>,
    /// Rendered line index within the chunk.
    pub(crate) line_index: usize,
    /// Zero-based occurrence ordinal for multiple matches on the same line.
    pub(crate) match_index_in_line: usize,
    /// True when the hit belongs to the in-flight live tail rather than committed history.
    pub(crate) is_live_tail: bool,
}

impl TranscriptSearchTarget {
    pub(crate) fn highlight_cell(self) -> Option<usize> {
        self.cell_index.filter(|_| !self.is_live_tail)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSearchDocument {
    chunk_index: usize,
    cell_index: Option<usize>,
    is_live_tail: bool,
    lines: Vec<String>,
}

impl TranscriptSearchDocument {
    pub(crate) fn committed_cell(cell_index: usize, lines: Vec<String>) -> Self {
        Self {
            chunk_index: cell_index,
            cell_index: Some(cell_index),
            is_live_tail: false,
            lines,
        }
    }

    pub(crate) fn live_tail(chunk_index: usize, lines: Vec<String>) -> Self {
        Self {
            chunk_index,
            cell_index: None,
            is_live_tail: true,
            lines,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSearchStatus {
    pub(crate) query: String,
    pub(crate) result_count: usize,
    /// 1-based ordinal of the selected result.
    pub(crate) current_ordinal: Option<usize>,
    pub(crate) current_target: Option<TranscriptSearchTarget>,
    /// True when the latest next/prev navigation wrapped around the result list.
    pub(crate) wrapped: bool,
}

impl TranscriptSearchStatus {
    pub(crate) fn render_text(&self) -> String {
        if self.query.is_empty() {
            return "Search transcript".to_string();
        }

        if self.result_count == 0 {
            return format!("Search: {}  no matches", self.query);
        }

        let current = self.current_ordinal.unwrap_or(0);
        if self.wrapped {
            format!(
                "Search: {}  {current}/{} (wrapped)",
                self.query, self.result_count
            )
        } else {
            format!("Search: {}  {current}/{}", self.query, self.result_count)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSearchOverlayState {
    pub(crate) status: TranscriptSearchStatus,
    pub(crate) current_chunk: Option<usize>,
    pub(crate) highlight_cell: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TranscriptSearchMatch {
    target: TranscriptSearchTarget,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptSearchState {
    open: bool,
    query: String,
    matches: Vec<TranscriptSearchMatch>,
    current_match: Option<usize>,
    last_nav_wrapped: bool,
}

impl TranscriptSearchState {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.last_nav_wrapped = false;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.last_nav_wrapped = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn status(&self) -> Option<TranscriptSearchStatus> {
        if !self.open {
            return None;
        }

        let current_target = self
            .current_match
            .and_then(|idx| self.matches.get(idx))
            .map(|entry| entry.target);

        Some(TranscriptSearchStatus {
            query: self.query.clone(),
            result_count: self.matches.len(),
            current_ordinal: self.current_match.map(|idx| idx + 1),
            current_target,
            wrapped: self.last_nav_wrapped,
        })
    }

    pub(crate) fn overlay_state(&self) -> Option<TranscriptSearchOverlayState> {
        let status = self.status()?;
        let current_target = status.current_target;
        Some(TranscriptSearchOverlayState {
            status,
            current_chunk: current_target.map(|target| target.chunk_index),
            highlight_cell: current_target.and_then(TranscriptSearchTarget::highlight_cell),
        })
    }

    pub(crate) fn set_query(
        &mut self,
        query: impl Into<String>,
        documents: &[TranscriptSearchDocument],
    ) -> Option<TranscriptSearchOverlayState> {
        self.open = true;
        self.query = query.into();
        self.reindex(documents);
        if !self.query.is_empty() && !self.matches.is_empty() {
            self.current_match = Some(0);
        }
        self.last_nav_wrapped = false;
        self.overlay_state()
    }

    pub(crate) fn refresh(
        &mut self,
        documents: &[TranscriptSearchDocument],
    ) -> Option<TranscriptSearchOverlayState> {
        if !self.open {
            return None;
        }
        self.reindex(documents);
        self.last_nav_wrapped = false;
        self.overlay_state()
    }

    pub(crate) fn next(
        &mut self,
        documents: &[TranscriptSearchDocument],
    ) -> Option<TranscriptSearchOverlayState> {
        self.reindex(documents);
        self.step(NavigationDirection::Next);
        self.overlay_state()
    }

    pub(crate) fn prev(
        &mut self,
        documents: &[TranscriptSearchDocument],
    ) -> Option<TranscriptSearchOverlayState> {
        self.reindex(documents);
        self.step(NavigationDirection::Prev);
        self.overlay_state()
    }

    fn reindex(&mut self, documents: &[TranscriptSearchDocument]) {
        let previous_target = self
            .current_match
            .and_then(|idx| self.matches.get(idx))
            .copied();
        let previous_index = self.current_match;

        if self.query.is_empty() {
            self.matches.clear();
            self.current_match = None;
            return;
        }

        self.matches = build_matches(&self.query, documents);
        self.current_match = restore_match_index(&self.matches, previous_target, previous_index);
    }

    fn step(&mut self, direction: NavigationDirection) {
        let len = self.matches.len();
        if len == 0 {
            self.current_match = None;
            self.last_nav_wrapped = false;
            return;
        }

        let (next_index, wrapped) = match (direction, self.current_match) {
            (NavigationDirection::Next, Some(index)) if index + 1 < len => (index + 1, false),
            (NavigationDirection::Next, Some(_)) => (0, true),
            (NavigationDirection::Next, None) => (0, false),
            (NavigationDirection::Prev, Some(index)) if index > 0 => (index - 1, false),
            (NavigationDirection::Prev, Some(_)) => (len - 1, true),
            (NavigationDirection::Prev, None) => (len - 1, false),
        };

        self.current_match = Some(next_index);
        self.last_nav_wrapped = wrapped;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavigationDirection {
    Next,
    Prev,
}

fn restore_match_index(
    matches: &[TranscriptSearchMatch],
    previous_target: Option<TranscriptSearchMatch>,
    previous_index: Option<usize>,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }

    if let Some(previous_target) = previous_target
        && let Some(index) = matches.iter().position(|entry| *entry == previous_target)
    {
        return Some(index);
    }

    previous_index
        .map(|index| index.min(matches.len().saturating_sub(1)))
        .or(Some(0))
}

fn build_matches(
    query: &str,
    documents: &[TranscriptSearchDocument],
) -> Vec<TranscriptSearchMatch> {
    let normalized_query = query.to_lowercase();
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for document in documents {
        for (line_index, line) in document.lines.iter().enumerate() {
            for match_index_in_line in line_match_ordinals(line, &normalized_query) {
                out.push(TranscriptSearchMatch {
                    target: TranscriptSearchTarget {
                        chunk_index: document.chunk_index,
                        cell_index: document.cell_index,
                        line_index,
                        match_index_in_line,
                        is_live_tail: document.is_live_tail,
                    },
                });
            }
        }
    }
    out
}

fn line_match_ordinals(line: &str, normalized_query: &str) -> Vec<usize> {
    let normalized_line = line.to_lowercase();
    let mut ordinals = Vec::new();
    let mut offset = 0usize;
    let mut ordinal = 0usize;
    while let Some(relative) = normalized_line[offset..].find(normalized_query) {
        ordinals.push(ordinal);
        offset += relative + normalized_query.len();
        ordinal += 1;
    }
    ordinals
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    fn docs() -> Vec<TranscriptSearchDocument> {
        vec![
            TranscriptSearchDocument::committed_cell(
                0,
                vec!["alpha beta".to_string(), "beta beta".to_string()],
            ),
            TranscriptSearchDocument::committed_cell(1, vec!["gamma".to_string()]),
            TranscriptSearchDocument::live_tail(2, vec!["tail beta".to_string()]),
        ]
    }

    #[test]
    fn query_builds_match_count_and_first_result() {
        let mut search = TranscriptSearchState::default();
        let state = search.set_query("beta", &docs()).expect("overlay state");

        assert_eq!(state.status.result_count, 4);
        assert_eq!(state.status.current_ordinal, Some(1));
        assert_eq!(
            state.highlight_cell,
            Some(0),
            "first result should select the first committed cell"
        );
    }

    #[test]
    fn next_and_prev_wrap_results() {
        let mut search = TranscriptSearchState::default();
        search.set_query("gamma", &docs());

        let next = search.next(&docs()).expect("next state");
        assert_eq!(next.status.current_ordinal, Some(1));
        assert!(next.status.wrapped, "single result should wrap on next");

        let prev = search.prev(&docs()).expect("prev state");
        assert_eq!(prev.status.current_ordinal, Some(1));
        assert!(prev.status.wrapped, "single result should wrap on prev");
    }

    #[test]
    fn prev_without_selection_lands_on_last_result() {
        let mut search = TranscriptSearchState::default();
        search.open();
        search.set_query("beta", &docs());
        search.current_match = None;

        let state = search.prev(&docs()).expect("prev state");
        assert_eq!(state.status.current_ordinal, Some(4));
        assert_eq!(state.current_chunk, Some(2));
        assert_eq!(state.highlight_cell, None);
    }

    #[test]
    fn status_text_reports_empty_and_no_match_states() {
        let mut search = TranscriptSearchState::default();
        search.open();
        assert_eq!(
            search.status().expect("status").render_text(),
            "Search transcript"
        );

        let status = search
            .set_query("zzz", &docs())
            .expect("overlay state")
            .status;
        assert_eq!(status.render_text(), "Search: zzz  no matches");
    }
}
