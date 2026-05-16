//! Small bounded in-app toast queue for the TUI.

use std::cmp::Reverse;
use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ToastSeverity {
    Info,
    Notice,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToastEntry {
    pub(crate) dedupe_key: String,
    pub(crate) message: String,
    pub(crate) severity: ToastSeverity,
    pub(crate) priority: u8,
    created_at: Instant,
    expires_at: Instant,
}

impl ToastEntry {
    pub(crate) fn new(
        dedupe_key: impl Into<String>,
        message: impl Into<String>,
        severity: ToastSeverity,
        priority: u8,
        duration: Duration,
    ) -> Self {
        let created_at = Instant::now();
        Self {
            dedupe_key: dedupe_key.into(),
            message: message.into(),
            severity,
            priority,
            created_at,
            expires_at: created_at + duration,
        }
    }

    pub(crate) fn remaining_duration(&self, now: Instant) -> Duration {
        self.expires_at.saturating_duration_since(now)
    }

    pub(crate) fn is_expired_at(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ToastQueue {
    entries: VecDeque<ToastEntry>,
    max_visible: usize,
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new(3)
    }
}

impl ToastQueue {
    pub(crate) fn new(max_visible: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_visible: max_visible.max(1),
        }
    }

    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let original_len = self.entries.len();
        self.entries.retain(|entry| !entry.is_expired_at(now));
        self.entries.len() != original_len
    }

    pub(crate) fn next_wakeup_in(&self, now: Instant) -> Option<Duration> {
        self.entries
            .iter()
            .map(|entry| entry.remaining_duration(now))
            .min()
    }

    pub(crate) fn visible_entries(&self) -> Vec<&ToastEntry> {
        self.entries.iter().take(self.max_visible).collect()
    }

    pub(crate) fn enqueue(&mut self, next: ToastEntry) {
        let now = Instant::now();
        self.expire(now);

        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.dedupe_key == next.dedupe_key)
        {
            let existing = self
                .entries
                .remove(position)
                .expect("position checked above");
            if existing.priority > next.priority {
                self.entries.push_front(existing);
                return;
            }
        }

        self.entries.push_front(next);
        let mut entries = self.entries.drain(..).collect::<Vec<_>>();
        entries.sort_by_key(|entry| (Reverse(entry.priority), Reverse(entry.created_at)));
        entries.truncate(self.max_visible);
        self.entries = entries.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn higher_priority_replaces_deduped_entry() {
        let mut queue = ToastQueue::new(3);
        queue.enqueue(ToastEntry::new(
            "copy",
            "Copied",
            ToastSeverity::Info,
            0,
            Duration::from_secs(2),
        ));
        queue.enqueue(ToastEntry::new(
            "copy",
            "Copy failed",
            ToastSeverity::Error,
            2,
            Duration::from_secs(2),
        ));

        let visible = queue.visible_entries();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].message, "Copy failed");
    }

    #[test]
    fn queue_is_bounded() {
        let mut queue = ToastQueue::new(2);
        queue.enqueue(ToastEntry::new(
            "a",
            "A",
            ToastSeverity::Info,
            0,
            Duration::from_secs(2),
        ));
        queue.enqueue(ToastEntry::new(
            "b",
            "B",
            ToastSeverity::Notice,
            1,
            Duration::from_secs(2),
        ));
        queue.enqueue(ToastEntry::new(
            "c",
            "C",
            ToastSeverity::Error,
            2,
            Duration::from_secs(2),
        ));

        assert_eq!(
            queue
                .visible_entries()
                .into_iter()
                .map(|entry| entry.message.clone())
                .collect::<Vec<_>>(),
            vec!["C".to_string(), "B".to_string()]
        );
    }
}
