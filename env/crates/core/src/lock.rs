//! Auto-lock timing. Pure: the caller passes the current [`Instant`], so
//! tests don't sleep.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct AutoLock {
    timeout: Duration,
    last_activity: Instant,
}

impl AutoLock {
    #[must_use]
    pub fn new(minutes: u32, now: Instant) -> Self {
        Self {
            timeout: minutes_to_duration(minutes),
            last_activity: now,
        }
    }

    /// Any keyboard or mouse activity.
    pub fn touch(&mut self, now: Instant) {
        self.last_activity = now;
    }

    pub fn set_minutes(&mut self, minutes: u32, now: Instant) {
        self.timeout = minutes_to_duration(minutes);
        self.last_activity = now;
    }

    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    #[must_use]
    pub fn remaining(&self, now: Instant) -> Duration {
        self.timeout
            .saturating_sub(now.saturating_duration_since(self.last_activity))
    }

    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        self.remaining(now).is_zero()
    }

    /// "Locks in 29 min" (whole minutes left, rounded down), "Locks in
    /// under a minute" in the last one.
    #[must_use]
    pub fn label(&self, now: Instant) -> String {
        let minutes = self.remaining(now).as_secs() / 60;
        if minutes == 0 {
            "Locks in under a minute".to_string()
        } else {
            format!("Locks in {minutes} min")
        }
    }
}

fn minutes_to_duration(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes.max(1)) * 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_and_resets_on_activity() {
        let start = Instant::now();
        let mut lock = AutoLock::new(30, start);
        assert_eq!(
            lock.label(start + Duration::from_secs(30)),
            "Locks in 29 min"
        );
        assert!(!lock.is_expired(start + Duration::from_secs(29 * 60)));
        assert_eq!(
            lock.label(start + Duration::from_secs(29 * 60 + 30)),
            "Locks in under a minute"
        );
        assert!(lock.is_expired(start + Duration::from_secs(30 * 60)));

        lock.touch(start + Duration::from_secs(20 * 60));
        assert!(!lock.is_expired(start + Duration::from_secs(30 * 60)));
        assert!(lock.is_expired(start + Duration::from_secs(50 * 60)));

        lock.set_minutes(5, start);
        assert_eq!(lock.timeout(), Duration::from_secs(300));
        assert!(lock.is_expired(start + Duration::from_secs(300)));
        // A clock that appears to go backwards never unlocks forever.
        assert_eq!(
            lock.remaining(start - Duration::from_secs(1)),
            lock.timeout()
        );
    }
}
