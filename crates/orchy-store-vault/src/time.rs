use std::sync::Mutex;

use chrono::{DateTime, Utc};
use orchy_core::{Clock, IdGenerator};
use ulid::{Generator, Ulid};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Monotonic ULIDs.
///
/// `Ulid::new()` fills the low bits randomly, so two ids minted in the same millisecond can
/// compare in either direction. orchy relies on id ordering in two places that would silently
/// break: the inbox watermark ("unread" is `id > mark`) and filename sort order. The crate's
/// `Generator` increments the random component instead when the timestamp has not advanced.
pub struct UlidGenerator(Mutex<Generator>);

impl UlidGenerator {
    pub fn new() -> Self {
        Self(Mutex::new(Generator::new()))
    }
}

impl Default for UlidGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdGenerator for UlidGenerator {
    fn generate(&self) -> Ulid {
        let mut generator = self.0.lock().expect("ulid generator mutex");
        // The generator only fails if the random component overflows within one millisecond,
        // which takes 2^80 ids; falling back to a fresh random ulid keeps that unreachable
        // case from panicking.
        generator.generate().unwrap_or_else(|_| Ulid::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_unique_and_monotonic_within_one_millisecond() {
        let ids = UlidGenerator::new();
        let mut previous = ids.generate();
        for _ in 0..10_000 {
            let next = ids.generate();
            assert!(
                next > previous,
                "ids must increase even when minted in the same millisecond: {previous} then {next}"
            );
            previous = next;
        }
    }

    #[test]
    fn the_system_clock_moves_forward() {
        let clock = SystemClock;
        assert!(clock.now() <= clock.now());
    }
}
