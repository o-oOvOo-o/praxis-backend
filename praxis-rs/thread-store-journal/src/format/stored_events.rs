use praxis_thread_store_contracts::ThreadEventEnvelope;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de::SeqAccess;
use serde::de::Visitor;
use serde::ser::SerializeSeq;
use std::fmt;
use std::ops::Deref;

#[derive(Clone, Debug)]
pub(super) enum StoredEvents {
    Empty,
    One(ThreadEventEnvelope),
    Many(Vec<ThreadEventEnvelope>),
}

impl StoredEvents {
    pub(super) fn preserve_allocation(events: Vec<ThreadEventEnvelope>) -> Self {
        Self::Many(events)
    }

    pub(super) fn into_vec(self) -> Vec<ThreadEventEnvelope> {
        match self {
            Self::Empty => Vec::new(),
            Self::One(event) => vec![event],
            Self::Many(events) => events,
        }
    }

    pub(super) fn consume(self) -> impl ExactSizeIterator<Item = ThreadEventEnvelope> {
        match self {
            Self::Empty => StoredEventsIter::Empty,
            Self::One(event) => StoredEventsIter::One(Some(event)),
            Self::Many(events) => StoredEventsIter::Many(events.into_iter()),
        }
    }
}

impl Deref for StoredEvents {
    type Target = [ThreadEventEnvelope];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Empty => &[],
            Self::One(event) => std::slice::from_ref(event),
            Self::Many(events) => events,
        }
    }
}

impl Serialize for StoredEvents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.len()))?;
        for event in self.iter() {
            sequence.serialize_element(event)?;
        }
        sequence.end()
    }
}

impl<'de> Deserialize<'de> for StoredEvents {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(StoredEventsVisitor)
    }
}

struct StoredEventsVisitor;

impl<'de> Visitor<'de> for StoredEventsVisitor {
    type Value = StoredEvents;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a thread event sequence")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let Some(first) = sequence.next_element()? else {
            return Ok(StoredEvents::Empty);
        };
        let Some(second) = sequence.next_element()? else {
            return Ok(StoredEvents::One(first));
        };
        let mut events =
            Vec::with_capacity(sequence.size_hint().unwrap_or_default().saturating_add(2));
        events.push(first);
        events.push(second);
        while let Some(event) = sequence.next_element()? {
            events.push(event);
        }
        Ok(StoredEvents::Many(events))
    }
}

enum StoredEventsIter {
    Empty,
    One(Option<ThreadEventEnvelope>),
    Many(std::vec::IntoIter<ThreadEventEnvelope>),
}

impl Iterator for StoredEventsIter {
    type Item = ThreadEventEnvelope;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::One(event) => event.take(),
            Self::Many(events) => events.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = match self {
            Self::Empty => 0,
            Self::One(event) => usize::from(event.is_some()),
            Self::Many(events) => events.len(),
        };
        (len, Some(len))
    }
}

impl ExactSizeIterator for StoredEventsIter {}
