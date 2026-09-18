use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::kind::{Kind, Status};
use crate::error::Result;
use crate::id::Id;
use crate::namespace::Namespace;
use crate::tag::Tag;

#[async_trait]
pub trait Search: Send + Sync {
    async fn sections(&self, query: &SearchQuery) -> Result<Vec<Hit>>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub kind: Option<Vec<Kind>>,
    pub status: Option<Vec<Status>>,
    pub namespace: Option<Namespace>,
    pub tags: Vec<Tag>,
    pub since: Option<DateTime<Utc>>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub document: Id,
    pub heading: Option<String>,
    pub excerpt: String,
    pub namespace: Namespace,
    pub updated_at: DateTime<Utc>,
    pub matches: usize,
}

pub fn rank(hits: &mut [Hit], anchor: Option<&Namespace>, now: DateTime<Utc>) {
    hits.sort_by(|a, b| {
        score(b, anchor, now)
            .total_cmp(&score(a, anchor, now))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.document.cmp(&b.document))
    });
}

fn score(hit: &Hit, anchor: Option<&Namespace>, now: DateTime<Utc>) -> f64 {
    let matches = (hit.matches as f64).min(10.0);
    let age_days = (now - hit.updated_at).num_days().max(0) as f64;
    let recency = 1.0 / (1.0 + age_days / 90.0);
    let proximity = match anchor {
        Some(anchor) if anchor == &hit.namespace => 2.0,
        Some(anchor) if anchor.contains(&hit.namespace) => 1.5,
        Some(_) => 1.0,
        None => 1.0,
    };
    matches * recency * proximity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(days_ago: i64, now: DateTime<Utc>) -> DateTime<Utc> {
        now - chrono::Duration::days(days_ago)
    }

    fn hit(id: &str, ns: &str, matches: usize, updated_at: DateTime<Utc>) -> Hit {
        Hit {
            document: Id::new(id).unwrap(),
            heading: None,
            excerpt: String::new(),
            namespace: Namespace::new(ns).unwrap(),
            updated_at,
            matches,
        }
    }

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
    const C: &str = "01CX5ZZKBKACTAV9WEVGEMMVRZ";

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn more_matches_outrank_fewer_when_all_else_is_equal() {
        let now = now();
        let mut hits = vec![hit(A, "/", 1, now), hit(B, "/", 5, now)];
        rank(&mut hits, None, now);
        assert_eq!(hits[0].document.to_string(), B);
    }

    #[test]
    fn a_recent_document_outranks_a_stale_one_with_the_same_match_count() {
        let now = now();
        let mut hits = vec![hit(A, "/", 3, at(365, now)), hit(B, "/", 3, at(1, now))];
        rank(&mut hits, None, now);
        assert_eq!(hits[0].document.to_string(), B, "recency breaks the tie");
    }

    #[test]
    fn the_anchor_namespace_is_preferred_over_an_unrelated_one() {
        let now = now();
        let anchor = Namespace::new("/backend").unwrap();
        let mut hits = vec![hit(A, "/frontend", 3, now), hit(B, "/backend", 3, now)];
        rank(&mut hits, Some(&anchor), now);
        assert_eq!(hits[0].document.to_string(), B);
    }

    #[test]
    fn a_child_of_the_anchor_ranks_between_the_anchor_and_a_stranger() {
        let now = now();
        let anchor = Namespace::new("/backend").unwrap();
        let mut hits = vec![
            hit(A, "/frontend", 3, now),
            hit(B, "/backend/auth", 3, now),
            hit(C, "/backend", 3, now),
        ];
        rank(&mut hits, Some(&anchor), now);
        let order: Vec<String> = hits.iter().map(|h| h.document.to_string()).collect();
        assert_eq!(order, vec![C.to_owned(), B.to_owned(), A.to_owned()]);
    }

    #[test]
    fn ranking_is_deterministic_for_identical_hits() {
        let now = now();
        let mut first = vec![hit(B, "/", 3, now), hit(A, "/", 3, now)];
        let mut second = vec![hit(A, "/", 3, now), hit(B, "/", 3, now)];
        rank(&mut first, None, now);
        rank(&mut second, None, now);
        assert_eq!(
            first
                .iter()
                .map(|h| h.document.to_string())
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|h| h.document.to_string())
                .collect::<Vec<_>>(),
            "a tie must break on id, not on input order"
        );
    }

    #[test]
    fn a_future_timestamp_does_not_produce_a_negative_score() {
        let now = now();
        let mut hits = vec![hit(A, "/", 1, now + chrono::Duration::days(10))];
        rank(&mut hits, None, now);
        assert!(score(&hits[0], None, now) > 0.0);
    }
}
