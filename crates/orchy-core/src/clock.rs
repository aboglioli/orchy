use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[test]
    fn a_clock_can_be_pinned_for_deterministic_replay() {
        let at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        assert_eq!(FixedClock(at).now(), at);
    }
}
