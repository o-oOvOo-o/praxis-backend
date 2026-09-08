use std::time::{Duration, Instant};

pub(super) struct ThreadSwitchTiming<I> {
    pub(super) target: I,
    started: Instant,
    first_frame: Option<Duration>,
    pub(super) cached: Option<bool>,
}

pub(super) struct FrameTiming {
    pub(super) elapsed: Duration,
    pub(super) first_frame: bool,
    pub(super) complete: bool,
}

impl<I: PartialEq> ThreadSwitchTiming<I> {
    pub(super) fn new(target: I, started: Instant) -> Self {
        Self {
            target,
            started,
            first_frame: None,
            cached: None,
        }
    }

    pub(super) fn frame(&mut self, target: &I, ready: bool, now: Instant) -> Option<FrameTiming> {
        if &self.target != target {
            return None;
        }
        let elapsed = now.saturating_duration_since(self.started);
        let first_frame = self.first_frame.is_none();
        if first_frame {
            self.first_frame = Some(elapsed);
        }
        (first_frame || ready).then_some(FrameTiming {
            elapsed,
            first_frame,
            complete: ready,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_frames_do_not_complete_a_switch() {
        let start = Instant::now();
        let mut timing = ThreadSwitchTiming::new(7, start);
        assert!(timing.frame(&8, true, start).is_none());
        let first = timing
            .frame(&7, false, start + Duration::from_millis(10))
            .unwrap();
        assert!(first.first_frame && !first.complete);
        assert_eq!(first.elapsed, Duration::from_millis(10));
        assert!(
            timing
                .frame(&7, false, start + Duration::from_millis(20))
                .is_none()
        );
        let ready = timing
            .frame(&7, true, start + Duration::from_millis(30))
            .unwrap();
        assert!(!ready.first_frame && ready.complete);
        assert_eq!(ready.elapsed, Duration::from_millis(30));
    }
    #[test]
    fn hot_frame_can_also_be_complete() {
        let start = Instant::now();
        let frame = ThreadSwitchTiming::new(7, start)
            .frame(&7, true, start)
            .unwrap();
        assert!(frame.first_frame && frame.complete);
    }
}
