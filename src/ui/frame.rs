use std::time::{Duration, Instant};

pub const MIN_FRAME_INTERVAL: Duration = Duration::from_nanos(8_333_334);
pub const ACTIVE_POLL_INTERVAL: Duration = Duration::from_millis(16);
pub const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Default)]
pub struct FrameClock {
    last_draw_at: Option<Instant>,
}

impl FrameClock {
    pub fn draw_due(&self, now: Instant) -> bool {
        match self.last_draw_at {
            Some(last) => now.duration_since(last) >= MIN_FRAME_INTERVAL,
            None => true,
        }
    }

    pub fn mark_drawn(&mut self, now: Instant) {
        self.last_draw_at = Some(now);
    }

    pub fn poll_timeout(&self, now: Instant, needs_draw: bool, live_activity: bool) -> Duration {
        if needs_draw {
            return self
                .last_draw_at
                .and_then(|last| {
                    last.checked_add(MIN_FRAME_INTERVAL)
                        .map(|deadline| deadline.saturating_duration_since(now))
                })
                .unwrap_or(Duration::ZERO);
        }

        if live_activity {
            ACTIVE_POLL_INTERVAL
        } else {
            IDLE_POLL_INTERVAL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_draw_is_due_immediately() {
        assert!(FrameClock::default().draw_due(Instant::now()));
    }

    #[test]
    fn draw_is_limited_until_min_interval_passes() {
        let t0 = Instant::now();
        let mut clock = FrameClock::default();
        clock.mark_drawn(t0);

        assert!(!clock.draw_due(t0 + Duration::from_millis(1)));
        assert!(clock.draw_due(t0 + MIN_FRAME_INTERVAL));
    }

    #[test]
    fn dirty_frame_timeout_waits_until_next_draw_deadline() {
        let t0 = Instant::now();
        let mut clock = FrameClock::default();
        clock.mark_drawn(t0);

        assert_eq!(
            clock.poll_timeout(t0 + Duration::from_millis(1), true, false),
            MIN_FRAME_INTERVAL - Duration::from_millis(1)
        );
    }

    #[test]
    fn clean_frame_timeout_tracks_activity() {
        let clock = FrameClock::default();
        let now = Instant::now();

        assert_eq!(clock.poll_timeout(now, false, true), ACTIVE_POLL_INTERVAL);
        assert_eq!(clock.poll_timeout(now, false, false), IDLE_POLL_INTERVAL);
    }
}
