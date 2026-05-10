use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSequence(u64);

impl EventSequence {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCursor {
    Sequence(EventSequence),
    Timestamp(DateTime<Utc>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_orders_by_value() {
        assert!(EventSequence::new(1) < EventSequence::new(2));
    }

    #[test]
    fn sequence_value_accessor() {
        assert_eq!(EventSequence::new(42).value(), 42);
    }

    #[test]
    fn cursor_variants_construct() {
        let _seq = EventCursor::Sequence(EventSequence::new(1));
        let _ts = EventCursor::Timestamp(Utc::now());
    }
}
