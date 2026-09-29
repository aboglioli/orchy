use chrono::{DateTime, Duration, Utc};

use crate::error::{CliError, CliResult};

/// An RFC 3339 timestamp, or a window back from now: `30m`, `2h`, `3d`, `1w`.
pub(crate) fn parse(input: &str, now: DateTime<Utc>) -> CliResult<DateTime<Utc>> {
    if let Ok(at) = DateTime::parse_from_rfc3339(input) {
        return Ok(at.with_timezone(&Utc));
    }
    let refused = || {
        CliError::config(format!(
            "`{input}` is neither a timestamp (2026-09-28T10:00:00Z) nor a window (30m, 2h, 3d, 1w)"
        ))
    };
    let (amount, unit) = input.split_at(input.len().saturating_sub(1));
    let amount: i64 = amount.parse().map_err(|_| refused())?;
    let window = match unit {
        "m" => Duration::minutes(amount),
        "h" => Duration::hours(amount),
        "d" => Duration::days(amount),
        "w" => Duration::weeks(amount),
        _ => return Err(refused()),
    };
    Ok(now - window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn a_window_counts_back_from_now() {
        assert_eq!(parse("2h", now()).unwrap(), now() - Duration::hours(2));
        assert_eq!(parse("3d", now()).unwrap(), now() - Duration::days(3));
    }

    #[test]
    fn a_timestamp_is_taken_as_given() {
        assert_eq!(
            parse("2026-09-01T00:00:00Z", now()).unwrap().to_rfc3339(),
            "2026-09-01T00:00:00+00:00"
        );
    }

    #[test]
    fn anything_else_is_refused_with_the_accepted_forms() {
        let err = parse("yesterday", now()).unwrap_err();
        assert!(err.to_string().contains("30m"), "{err}");
    }
}
