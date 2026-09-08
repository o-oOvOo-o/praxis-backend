#[derive(Debug)]
pub(crate) struct PendingNavigation<R, T> {
    request: Option<(R, T, Option<T>)>,
}

impl<R, T> Default for PendingNavigation<R, T> {
    fn default() -> Self {
        Self { request: None }
    }
}

impl<R: PartialEq, T: PartialEq> PendingNavigation<R, T> {
    pub(crate) fn begin(&mut self, request: R, target: T, origin: Option<T>) {
        self.request = Some((request, target, origin));
    }

    pub(crate) fn target(&self) -> Option<&T> {
        self.request.as_ref().map(|(_, target, _)| target)
    }

    pub(crate) fn clear(&mut self) {
        self.request = None;
    }

    pub(crate) fn accept(&mut self, request: &R, target: &T, current: Option<&T>) -> bool {
        let Some((expected, pending_target, origin)) = &self.request else {
            return false;
        };
        if expected != request || pending_target != target {
            return false;
        }
        let current_origin = origin.as_ref() == current;
        self.clear();
        current_origin
    }
}

#[cfg(test)]
mod tests {
    use super::PendingNavigation;

    #[test]
    fn late_completion_cannot_replace_newer_selection_even_for_same_target() {
        let mut pending = PendingNavigation::default();
        pending.begin(1, "B", Some("A"));
        pending.begin(2, "C", Some("A"));
        pending.begin(3, "B", Some("A"));
        assert!(!pending.accept(&1, &"B", Some(&"A")));
        assert!(!pending.accept(&2, &"C", Some(&"A")));
        assert_eq!(pending.target(), Some(&"B"));
        assert!(pending.accept(&3, &"B", Some(&"A")));
        assert!(!pending.accept(&3, &"B", Some(&"A")));
    }

    #[test]
    fn returning_to_current_thread_cancels_pending_navigation() {
        let mut pending = PendingNavigation::default();
        pending.begin(1, "B", Some("A"));
        pending.clear();
        assert!(!pending.accept(&1, &"B", Some(&"A")));
        assert_eq!(pending.target(), None);
    }

    #[test]
    fn other_navigation_invalidates_completion() {
        let mut pending = PendingNavigation::default();
        pending.begin(1, "B", Some("A"));
        assert!(!pending.accept(&1, &"B", Some(&"C")));
        assert_eq!(pending.target(), None);
    }
}
