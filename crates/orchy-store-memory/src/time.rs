use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use orchy_core::{Clock, IdGenerator};
use ulid::Ulid;

pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    pub fn at(epoch_seconds: i64) -> Self {
        Self(Mutex::new(
            DateTime::from_timestamp(epoch_seconds, 0).expect("valid epoch seconds"),
        ))
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.0.lock().expect("clock mutex");
        *now += by;
    }

    pub fn set(&self, to: DateTime<Utc>) {
        *self.0.lock().expect("clock mutex") = to;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock mutex")
    }
}

pub struct SeqIdGenerator(AtomicU64);

impl SeqIdGenerator {
    pub fn new() -> Self {
        Self(AtomicU64::new(1))
    }
}

impl Default for SeqIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdGenerator for SeqIdGenerator {
    fn generate(&self) -> Ulid {
        let n = self.0.fetch_add(1, Ordering::SeqCst);
        Ulid::from_parts(n, n as u128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_clock_only_moves_when_told_to() {
        let clock = FixedClock::at(1000);
        let first = clock.now();
        assert_eq!(
            clock.now(),
            first,
            "time must not drift under a fixed clock"
        );
        clock.advance(Duration::seconds(60));
        assert_eq!(clock.now(), first + Duration::seconds(60));
    }

    #[test]
    fn sequential_ids_are_unique_and_ordered() {
        let ids = SeqIdGenerator::new();
        let a = ids.generate();
        let b = ids.generate();
        assert_ne!(a, b);
        assert!(a < b, "ids must sort in generation order");
    }
}
