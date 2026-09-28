mod terms;

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

pub use terms::tokenise;

use crate::document::{DocumentStatus, Kind};
use crate::entity_ref::{EntityKind, EntityRef};
use crate::error::Result;
use crate::namespace::Namespace;
use crate::tag::Tag;

const K1: f64 = 1.2;
const B: f64 = 0.75;
const TITLE_WEIGHT: f64 = 3.0;
const PHRASE_BONUS: f64 = 1.5;

#[async_trait]
pub trait Search: Send + Sync {
    async fn sections(&self, query: &SearchQuery) -> Result<Vec<Hit>>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub text: String,
    pub entities: Option<Vec<EntityKind>>,
    pub kind: Option<Vec<Kind>>,
    pub status: Option<Vec<DocumentStatus>>,
    pub retired: bool,
    pub namespace: Option<Namespace>,
    pub tags: Vec<Tag>,
    pub since: Option<DateTime<Utc>>,
    pub limit: usize,
}

impl SearchQuery {
    pub fn covers(&self, kind: EntityKind) -> bool {
        self.entities.as_ref().is_none_or(|k| k.contains(&kind))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    pub entity: EntityRef,
    pub heading: Option<String>,
    pub title: String,
    pub body: String,
    pub excerpt: String,
    pub namespace: Namespace,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub entity: EntityRef,
    pub heading: Option<String>,
    pub excerpt: String,
    pub namespace: Namespace,
    pub updated_at: DateTime<Utc>,
    pub relevance: f64,
}

impl Passage {
    fn into_hit(self, relevance: f64) -> Hit {
        Hit {
            entity: self.entity,
            heading: self.heading,
            excerpt: self.excerpt,
            namespace: self.namespace,
            updated_at: self.updated_at,
            relevance,
        }
    }

    fn holds_phrase(&self, needle: &str) -> bool {
        self.title.to_lowercase().contains(needle) || self.body.to_lowercase().contains(needle)
    }
}

struct Indexed {
    title: Vec<String>,
    body: Vec<String>,
}

impl Indexed {
    fn of(passage: &Passage) -> Self {
        Self {
            title: tokenise(&passage.title),
            body: tokenise(&passage.body),
        }
    }

    fn length(&self) -> f64 {
        (self.title.len() + self.body.len()) as f64
    }

    fn weighted_frequency(&self, term: &str) -> f64 {
        let occurrences = |tokens: &[String]| tokens.iter().filter(|t| *t == term).count() as f64;
        TITLE_WEIGHT * occurrences(&self.title) + occurrences(&self.body)
    }

    fn holds(&self, term: &str) -> bool {
        self.title.iter().any(|t| t == term) || self.body.iter().any(|t| t == term)
    }
}

pub fn score(passages: Vec<Passage>, text: &str) -> Vec<Hit> {
    let wanted: BTreeSet<String> = tokenise(text).into_iter().collect();
    if wanted.is_empty() {
        return passages.into_iter().map(|p| p.into_hit(0.0)).collect();
    }

    let indexed: Vec<Indexed> = passages.iter().map(Indexed::of).collect();
    let total = indexed.len() as f64;
    let average_length = (indexed.iter().map(Indexed::length).sum::<f64>() / total).max(1.0);

    let carrying: BTreeMap<&String, f64> = wanted
        .iter()
        .map(|term| {
            (
                term,
                indexed.iter().filter(|doc| doc.holds(term)).count() as f64,
            )
        })
        .collect();

    let needle = text.trim().to_lowercase();

    passages
        .into_iter()
        .zip(indexed)
        .filter_map(|(passage, doc)| {
            let mut relevance = 0.0;
            let mut matched = 0usize;

            for term in &wanted {
                let frequency = doc.weighted_frequency(term);
                if frequency == 0.0 {
                    continue;
                }
                matched += 1;
                let documents = carrying[term];
                let rarity = (1.0 + (total - documents + 0.5) / (documents + 0.5)).ln();
                let saturation =
                    frequency / (frequency + K1 * (1.0 - B + B * doc.length() / average_length));
                relevance += rarity * (K1 + 1.0) * saturation;
            }

            if matched == 0 {
                return None;
            }
            let coverage = matched as f64 / wanted.len() as f64;
            let phrase = if passage.holds_phrase(&needle) {
                PHRASE_BONUS
            } else {
                1.0
            };
            Some(passage.into_hit(relevance * coverage * phrase))
        })
        .collect()
}

pub fn rank(hits: &mut [Hit], anchor: Option<&Namespace>, now: DateTime<Utc>) {
    hits.sort_by(|a, b| {
        weight(b, anchor, now)
            .total_cmp(&weight(a, anchor, now))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.entity.id().cmp(b.entity.id()))
    });
}

fn weight(hit: &Hit, anchor: Option<&Namespace>, now: DateTime<Utc>) -> f64 {
    let age_days = (now - hit.updated_at).num_days().max(0) as f64;
    let recency = 1.0 / (1.0 + age_days / 90.0);
    let proximity = match anchor {
        Some(anchor) if anchor == &hit.namespace => 2.0,
        Some(anchor) if anchor.contains(&hit.namespace) => 1.5,
        _ => 1.0,
    };
    hit.relevance.max(f64::MIN_POSITIVE) * recency * proximity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;

    fn at(days_ago: i64, now: DateTime<Utc>) -> DateTime<Utc> {
        now - chrono::Duration::days(days_ago)
    }

    fn hit(id: &str, ns: &str, relevance: f64, updated_at: DateTime<Utc>) -> Hit {
        Hit {
            entity: EntityRef::new(EntityKind::Document, Id::new(id).unwrap()),
            heading: None,
            excerpt: String::new(),
            namespace: Namespace::new(ns).unwrap(),
            updated_at,
            relevance,
        }
    }

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
    const C: &str = "01CX5ZZKBKACTAV9WEVGEMMVRZ";

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn a_more_relevant_hit_outranks_a_less_relevant_one_when_all_else_is_equal() {
        let now = now();
        let mut hits = vec![hit(A, "/", 1.0, now), hit(B, "/", 5.0, now)];
        rank(&mut hits, None, now);
        assert_eq!(hits[0].entity.id().to_string(), B);
    }

    #[test]
    fn a_recent_document_outranks_a_stale_one_of_equal_relevance() {
        let now = now();
        let mut hits = vec![hit(A, "/", 3.0, at(365, now)), hit(B, "/", 3.0, at(1, now))];
        rank(&mut hits, None, now);
        assert_eq!(hits[0].entity.id().to_string(), B, "recency breaks the tie");
    }

    #[test]
    fn the_anchor_namespace_is_preferred_over_an_unrelated_one() {
        let now = now();
        let anchor = Namespace::new("/backend").unwrap();
        let mut hits = vec![hit(A, "/frontend", 3.0, now), hit(B, "/backend", 3.0, now)];
        rank(&mut hits, Some(&anchor), now);
        assert_eq!(hits[0].entity.id().to_string(), B);
    }

    #[test]
    fn a_child_of_the_anchor_ranks_between_the_anchor_and_a_stranger() {
        let now = now();
        let anchor = Namespace::new("/backend").unwrap();
        let mut hits = vec![
            hit(A, "/frontend", 3.0, now),
            hit(B, "/backend/auth", 3.0, now),
            hit(C, "/backend", 3.0, now),
        ];
        rank(&mut hits, Some(&anchor), now);
        let order: Vec<String> = hits.iter().map(|h| h.entity.id().to_string()).collect();
        assert_eq!(order, vec![C.to_owned(), B.to_owned(), A.to_owned()]);
    }

    #[test]
    fn ranking_is_deterministic_for_identical_hits() {
        let now = now();
        let mut first = vec![hit(B, "/", 3.0, now), hit(A, "/", 3.0, now)];
        let mut second = vec![hit(A, "/", 3.0, now), hit(B, "/", 3.0, now)];
        rank(&mut first, None, now);
        rank(&mut second, None, now);
        assert_eq!(
            first
                .iter()
                .map(|h| h.entity.id().to_string())
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|h| h.entity.id().to_string())
                .collect::<Vec<_>>(),
            "a tie must break on id, not on input order"
        );
    }

    #[test]
    fn a_future_timestamp_does_not_produce_a_negative_score() {
        let now = now();
        let mut hits = vec![hit(A, "/", 1.0, now + chrono::Duration::days(10))];
        rank(&mut hits, None, now);
        assert!(weight(&hits[0], None, now) > 0.0);
    }
}

#[cfg(test)]
mod scoring_tests {
    use super::*;
    use crate::id::Id;

    fn at(n: u8) -> Id {
        Id::new(format!("01ARZ3NDEKTSV4RRFFQ69G5F{n:02}")).unwrap()
    }

    fn passage(n: u8, title: &str, body: &str) -> Passage {
        Passage {
            entity: EntityRef::new(EntityKind::Document, at(n)),
            heading: None,
            title: title.to_owned(),
            body: body.to_owned(),
            excerpt: body.to_owned(),
            namespace: Namespace::root(),
            updated_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        }
    }

    fn relevance_of(hits: &[Hit], n: u8) -> f64 {
        hits.iter()
            .find(|h| h.entity.id() == &at(n))
            .map(|h| h.relevance)
            .unwrap_or(0.0)
    }

    #[test]
    fn query_terms_are_matched_independently_of_each_other_and_of_order() {
        let found = passage(
            1,
            "runbook",
            "A long migration holds the lock and blocks every deploy",
        );

        for query in ["deploy lock", "lock deploy", "blocks migration deploy"] {
            let hits = score(vec![found.clone()], query);
            assert_eq!(hits.len(), 1, "`{query}` should still find it");
        }
    }

    #[test]
    fn a_term_is_found_through_its_inflections() {
        let found = passage(1, "policy", "never edit an applied migration");

        for query in ["migrate", "migrations", "migrating", "MIGRATION"] {
            assert_eq!(score(vec![found.clone()], query).len(), 1, "`{query}`");
        }
    }

    #[test]
    fn a_passage_carrying_every_term_outranks_one_carrying_some() {
        let hits = score(
            vec![
                passage(1, "one", "a migration can stall a deploy"),
                passage(2, "two", "a migration is a migration is a migration"),
            ],
            "migration deploy",
        );
        assert!(
            relevance_of(&hits, 1) > relevance_of(&hits, 2),
            "covering both terms beats repeating one of them"
        );
    }

    #[test]
    fn a_term_almost_everything_carries_counts_for_less_than_a_rare_one() {
        let mut corpus: Vec<Passage> = (1..=9)
            .map(|n| passage(n, "note", "the deploy pipeline runs the deploy"))
            .collect();
        corpus.push(passage(10, "note", "the deploy pipeline runs a migration"));

        let hits = score(corpus, "deploy migration");
        assert_eq!(
            hits.iter()
                .max_by(|a, b| a.relevance.total_cmp(&b.relevance))
                .unwrap()
                .entity
                .id()
                .clone(),
            at(10),
            "the one term that distinguishes a passage is worth more than the one they share"
        );
    }

    #[test]
    fn a_term_in_the_title_weighs_more_than_the_same_term_in_a_body() {
        let hits = score(
            vec![
                passage(
                    1,
                    "migration safety",
                    "unrelated prose of similar length here",
                ),
                passage(
                    2,
                    "unrelated title",
                    "this mentions migration once in passing",
                ),
            ],
            "migration",
        );
        assert!(relevance_of(&hits, 1) > relevance_of(&hits, 2));
    }

    #[test]
    fn a_long_passage_does_not_win_on_length_alone() {
        let padding = "filler words that say nothing at all ".repeat(40);
        let hits = score(
            vec![
                passage(1, "short", "migration"),
                passage(2, "long", &format!("migration {padding}")),
            ],
            "migration",
        );
        assert!(
            relevance_of(&hits, 1) > relevance_of(&hits, 2),
            "length normalisation stops a wall of text outranking a direct answer"
        );
    }

    #[test]
    fn the_exact_phrase_is_preferred_over_the_same_words_scattered() {
        let hits = score(
            vec![
                passage(1, "note", "never edit an applied migration"),
                passage(2, "note", "migration notes: do not edit what was applied"),
            ],
            "applied migration",
        );
        assert!(
            relevance_of(&hits, 1) > relevance_of(&hits, 2),
            "the phrase as typed is a stronger signal than its words apart"
        );
    }

    #[test]
    fn a_passage_carrying_none_of_the_terms_is_not_a_hit() {
        let hits = score(
            vec![passage(
                1,
                "frontend",
                "design tokens are generated at build time",
            )],
            "migration",
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn an_empty_query_browses_rather_than_searches() {
        let corpus = vec![passage(1, "a", "one"), passage(2, "b", "two")];
        let hits = score(corpus, "   ");
        assert_eq!(hits.len(), 2, "everything is returned");
        assert!(hits.iter().all(|h| h.relevance == 0.0), "and nothing ranks");
    }

    #[test]
    fn relevance_survives_ranking_so_a_better_match_still_comes_first() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut hits = score(
            vec![
                passage(1, "note", "migration once"),
                passage(2, "migration safety", "migration twice migration"),
            ],
            "migration",
        );
        rank(&mut hits, None, now);
        assert_eq!(hits[0].entity.id(), &at(2));
    }
}
