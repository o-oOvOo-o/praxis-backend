use crate::unified_exec::UNIFIED_EXEC_OUTPUT_MAX_BYTES;
use std::collections::VecDeque;

/// A capped byte buffer that preserves a stable prefix and the newest suffix.
///
/// Storage is deliberately independent of producer chunking: the prefix uses
/// one contiguous allocation and the suffix uses one byte ring. This keeps
/// memory bounded while avoiding an allocation for every process output chunk.
#[derive(Debug)]
pub(crate) struct HeadTailBuffer {
    head_budget: usize,
    tail_budget: usize,
    head: Vec<u8>,
    tail: VecDeque<u8>,
    omitted_bytes: usize,
}

impl Default for HeadTailBuffer {
    fn default() -> Self {
        Self::new(UNIFIED_EXEC_OUTPUT_MAX_BYTES)
    }
}

impl HeadTailBuffer {
    pub(crate) fn new(max_bytes: usize) -> Self {
        let head_budget = max_bytes / 2;
        let tail_budget = max_bytes.saturating_sub(head_budget);
        Self {
            head_budget,
            tail_budget,
            head: Vec::new(),
            tail: VecDeque::new(),
            omitted_bytes: 0,
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.head.len().saturating_add(self.tail.len())
    }

    #[cfg(test)]
    pub(crate) fn omitted_bytes(&self) -> usize {
        self.omitted_bytes
    }

    pub(crate) fn push_chunk(&mut self, chunk: Vec<u8>) {
        if chunk.is_empty() {
            return;
        }

        let head_space = self.head_budget.saturating_sub(self.head.len());
        let split = head_space.min(chunk.len());
        if split == chunk.len() && self.head.is_empty() {
            self.head = chunk;
            return;
        }
        self.head.extend_from_slice(&chunk[..split]);
        self.push_to_tail(&chunk[split..]);
    }

    pub(crate) fn snapshot_chunks(&self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::with_capacity(2);
        if !self.head.is_empty() {
            chunks.push(self.head.clone());
        }
        if !self.tail.is_empty() {
            chunks.push(self.tail.iter().copied().collect());
        }
        chunks
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.retained_bytes());
        bytes.extend_from_slice(&self.head);
        bytes.extend(self.tail.iter().copied());
        bytes
    }

    pub(crate) fn drain_chunks(&mut self) -> Vec<Vec<u8>> {
        let mut chunks = Vec::with_capacity(2);
        let head = std::mem::take(&mut self.head);
        if !head.is_empty() {
            chunks.push(head);
        }
        if !self.tail.is_empty() {
            chunks.push(self.tail.drain(..).collect());
        }
        self.omitted_bytes = 0;
        chunks
    }

    fn push_to_tail(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if self.tail_budget == 0 {
            self.omitted_bytes = self.omitted_bytes.saturating_add(bytes.len());
            return;
        }
        if bytes.len() >= self.tail_budget {
            let keep_from = bytes.len() - self.tail_budget;
            self.omitted_bytes = self
                .omitted_bytes
                .saturating_add(self.tail.len())
                .saturating_add(keep_from);
            self.tail.clear();
            self.tail.extend(&bytes[keep_from..]);
            return;
        }

        self.tail.extend(bytes);
        let overflow = self.tail.len().saturating_sub(self.tail_budget);
        if overflow != 0 {
            self.tail.drain(..overflow);
            self.omitted_bytes = self.omitted_bytes.saturating_add(overflow);
        }
    }
}

#[cfg(test)]
#[path = "head_tail_buffer_tests.rs"]
mod tests;
