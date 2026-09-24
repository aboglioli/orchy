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

/// `Ulid::new()` randomises the low bits, so two ids minted in the same millisecond can
/// compare either way. The inbox watermark (`id > mark`) and filename order both depend on
/// that comparison, so generation has to be monotonic.
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
