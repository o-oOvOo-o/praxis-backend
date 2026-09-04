use std::collections::VecDeque;

use super::ThreadBufferedEvent;
use super::ThreadEventEnvelope;
use super::cached_surface_event;
use super::event_requires_view_replay;
use super::merge_adjacent_stream_event;

const EVENT_LIMIT: usize = 256;

#[derive(Debug)]
pub(super) struct CachedViewDeltaLog {
    base_revision: u64,
    events: VecDeque<ThreadEventEnvelope>,
    surface_dirty: bool,
    overflowed: bool,
}

pub(in crate::app) struct CachedViewDelta {
    pub(in crate::app) events: VecDeque<ThreadEventEnvelope>,
    pub(in crate::app) surface_dirty: bool,
}

impl CachedViewDeltaLog {
    pub(super) fn new(base_revision: u64) -> Self {
        Self {
            base_revision,
            events: VecDeque::new(),
            surface_dirty: false,
            overflowed: false,
        }
    }

    pub(super) fn tracks(&self, base_revision: u64) -> bool {
        self.base_revision == base_revision && !self.overflowed
    }

    pub(super) fn finish(self, base_revision: u64) -> Option<CachedViewDelta> {
        self.tracks(base_revision).then_some(CachedViewDelta {
            events: self.events,
            surface_dirty: self.surface_dirty,
        })
    }

    pub(super) fn record(&mut self, sequence: u64, event: &ThreadBufferedEvent) {
        if cached_surface_event(event) {
            self.surface_dirty = true;
        }
        if !event_requires_view_replay(event) || self.overflowed {
            return;
        }
        if let Some(buffered) = self.events.back_mut()
            && merge_adjacent_stream_event(&mut buffered.event, event)
        {
            buffered.sequence = sequence;
            return;
        }
        if self.events.len() == EVENT_LIMIT {
            self.events.clear();
            self.overflowed = true;
            return;
        }
        self.events.push_back(ThreadEventEnvelope {
            sequence,
            event: event.clone(),
        });
    }
}
