use std::sync::Arc;

use ratatui::text::Line;

use super::ActiveCellTranscriptKey;
use super::ChatWidget;
use crate::history_cell;
use crate::history_cell::HistoryCell;
use crate::transcript_search::TranscriptSearchDocument;
use crate::transcript_search::TranscriptSearchOverlayState;
use crate::transcript_search::TranscriptSearchStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TranscriptSearchDocumentKey {
    width: u16,
    committed_len: usize,
    committed_first_ptr: usize,
    committed_last_ptr: usize,
    presentation_revision: u64,
    active_revision: Option<u64>,
    active_presentation_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptSearchDocumentCache {
    key: TranscriptSearchDocumentKey,
    documents: Vec<TranscriptSearchDocument>,
}

impl ChatWidget {
    pub(crate) fn open_transcript_search(&mut self) -> Option<TranscriptSearchStatus> {
        self.transcript_search.open();
        self.transcript_search.status()
    }

    pub(crate) fn close_transcript_search(&mut self) {
        self.transcript_search.close();
    }

    pub(crate) fn is_transcript_search_open(&self) -> bool {
        self.transcript_search.is_open()
    }

    pub(crate) fn transcript_search_query(&self) -> &str {
        self.transcript_search.query()
    }

    #[cfg(test)]
    pub(crate) fn transcript_search_status(&self) -> Option<TranscriptSearchStatus> {
        self.transcript_search.status()
    }

    pub(crate) fn set_transcript_search_query(
        &mut self,
        query: impl Into<String>,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Option<TranscriptSearchOverlayState> {
        let query = query.into();
        if query.is_empty() {
            self.transcript_search_document_cache = None;
            return self.transcript_search.set_query(query, &[]);
        }

        self.ensure_transcript_search_document_cache(width, transcript_cells);
        let documents = self
            .transcript_search_document_cache
            .as_ref()
            .map(|cache| cache.documents.as_slice())
            .unwrap_or(&[]);
        self.transcript_search.set_query(query, documents)
    }

    pub(crate) fn refresh_transcript_search(
        &mut self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Option<TranscriptSearchOverlayState> {
        self.refresh_transcript_search_if_stale(width, transcript_cells)
    }

    pub(crate) fn transcript_search_next(
        &mut self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Option<TranscriptSearchOverlayState> {
        let _ = self.refresh_transcript_search_if_stale(width, transcript_cells);
        self.transcript_search.next_indexed()
    }

    pub(crate) fn transcript_search_prev(
        &mut self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Option<TranscriptSearchOverlayState> {
        let _ = self.refresh_transcript_search_if_stale(width, transcript_cells);
        self.transcript_search.prev_indexed()
    }

    pub(crate) fn transcript_search_overlay_state(
        &mut self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Option<TranscriptSearchOverlayState> {
        self.refresh_transcript_search(width, transcript_cells)
    }

    fn refresh_transcript_search_if_stale(
        &mut self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Option<TranscriptSearchOverlayState> {
        if !self.transcript_search.is_open() {
            return None;
        }
        if self.transcript_search.query().is_empty() {
            return self.transcript_search.overlay_state();
        }

        let key = self.compute_transcript_search_document_key(width, transcript_cells);
        if self
            .transcript_search_document_cache
            .as_ref()
            .is_some_and(|cache| cache.key == key)
        {
            return self.transcript_search.overlay_state();
        }

        self.ensure_transcript_search_document_cache_for_key(key, width, transcript_cells);
        let documents = self
            .transcript_search_document_cache
            .as_ref()
            .map(|cache| cache.documents.as_slice())
            .unwrap_or(&[]);
        self.transcript_search.refresh(documents)
    }

    pub(crate) fn active_cell_transcript_key(&self) -> Option<ActiveCellTranscriptKey> {
        let cell = self.active_cell.as_ref()?;
        Some(ActiveCellTranscriptKey {
            revision: self.active_cell_revision,
            is_stream_continuation: cell.is_stream_continuation(),
            animation_tick: cell.transcript_animation_tick(),
            presentation_revision: history_cell::history_presentation_revision(),
        })
    }

    pub(crate) fn active_cell_transcript_lines(&self, width: u16) -> Option<Vec<Line<'static>>> {
        let cell = self.active_cell.as_ref()?;
        let lines = cell.transcript_lines(width);
        (!lines.is_empty()).then_some(lines)
    }

    fn transcript_search_documents(
        &self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> Vec<TranscriptSearchDocument> {
        let mut documents = transcript_cells
            .iter()
            .enumerate()
            .map(|(cell_index, cell)| {
                TranscriptSearchDocument::committed_cell(
                    cell_index,
                    Self::transcript_search_lines_to_strings(cell.transcript_lines(width)),
                )
            })
            .collect::<Vec<_>>();

        if let Some(active_lines) = self.active_cell_transcript_lines(width) {
            documents.push(TranscriptSearchDocument::live_tail(
                transcript_cells.len(),
                Self::transcript_search_lines_to_strings(active_lines),
            ));
        }

        documents
    }

    fn ensure_transcript_search_document_cache(
        &mut self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) {
        let key = self.compute_transcript_search_document_key(width, transcript_cells);
        self.ensure_transcript_search_document_cache_for_key(key, width, transcript_cells);
    }

    fn ensure_transcript_search_document_cache_for_key(
        &mut self,
        key: TranscriptSearchDocumentKey,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) {
        if self
            .transcript_search_document_cache
            .as_ref()
            .is_some_and(|cache| cache.key == key)
        {
            return;
        }

        self.transcript_search_document_cache = Some(TranscriptSearchDocumentCache {
            key,
            documents: self.transcript_search_documents(width, transcript_cells),
        });
    }

    fn compute_transcript_search_document_key(
        &self,
        width: u16,
        transcript_cells: &[Arc<dyn HistoryCell>],
    ) -> TranscriptSearchDocumentKey {
        let active_key = self.active_cell_transcript_key();
        TranscriptSearchDocumentKey {
            width,
            committed_len: transcript_cells.len(),
            committed_first_ptr: transcript_cells
                .first()
                .map(Self::history_cell_ptr_id)
                .unwrap_or_default(),
            committed_last_ptr: transcript_cells
                .last()
                .map(Self::history_cell_ptr_id)
                .unwrap_or_default(),
            presentation_revision: history_cell::history_presentation_revision(),
            active_revision: active_key.map(|key| key.revision),
            active_presentation_revision: active_key.map(|key| key.presentation_revision),
        }
    }

    fn transcript_search_lines_to_strings(lines: Vec<Line<'static>>) -> Vec<String> {
        lines
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<String>()
            })
            .collect()
    }

    fn history_cell_ptr_id(cell: &Arc<dyn HistoryCell>) -> usize {
        Arc::as_ptr(cell) as *const () as usize
    }
}
