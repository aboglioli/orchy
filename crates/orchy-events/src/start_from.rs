use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartFrom {
    Earliest,
    #[default]
    Latest,
    Timestamp(DateTime<Utc>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_latest() {
        assert_eq!(StartFrom::default(), StartFrom::Latest);
    }

    #[test]
    fn copy_and_eq_derive() {
        let a = StartFrom::Earliest;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn timestamp_variant() {
        let t = Utc::now();
        let s = StartFrom::Timestamp(t);
        if let StartFrom::Timestamp(t2) = s {
            assert_eq!(t, t2);
        } else {
            panic!("expected timestamp variant");
        }
    }
}
