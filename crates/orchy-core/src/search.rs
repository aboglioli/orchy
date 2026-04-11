use std::collections::HashMap;
use std::hash::Hash;

const RRF_K: f64 = 60.0;

/// Reciprocal Rank Fusion — merges two ranked result lists.
/// Each result is scored as `1 / (k + rank)` per list, then summed.
/// Returns top `limit` results sorted by combined score descending.
pub fn reciprocal_rank_fusion<K: Eq + Hash + Clone, T: Clone>(
    keyword_results: &[(K, T)],
    vector_results: &[(K, T)],
    limit: usize,
) -> Vec<T> {
    let mut scores: HashMap<K, (f64, Option<T>)> = HashMap::new();

    for (rank, (key, item)) in keyword_results.iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores.entry(key.clone()).or_insert((0.0, None));
        entry.0 += score;
        if entry.1.is_none() {
            entry.1 = Some(item.clone());
        }
    }

    for (rank, (key, item)) in vector_results.iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores.entry(key.clone()).or_insert((0.0, None));
        entry.0 += score;
        if entry.1.is_none() {
            entry.1 = Some(item.clone());
        }
    }

    let mut combined: Vec<(f64, T)> = scores
        .into_values()
        .filter_map(|(score, item)| item.map(|i| (score, i)))
        .collect();

    combined.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    combined.truncate(limit);
    combined.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_merges_two_lists() {
        let keyword: Vec<(u32, &str)> = vec![(1, "a"), (2, "b"), (3, "c")];
        let vector: Vec<(u32, &str)> = vec![(2, "b"), (4, "d"), (1, "a")];

        let results = reciprocal_rank_fusion(&keyword, &vector, 3);

        // "b" appears at rank 1 in keyword (score 1/62) and rank 0 in vector (score 1/61)
        // "a" appears at rank 0 in keyword (score 1/61) and rank 2 in vector (score 1/63)
        // "b" should have highest combined score
        assert_eq!(results[0], "b");
        assert_eq!(results[1], "a");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn rrf_respects_limit() {
        let keyword: Vec<(u32, &str)> = vec![(1, "a"), (2, "b"), (3, "c")];
        let vector: Vec<(u32, &str)> = vec![];

        let results = reciprocal_rank_fusion(&keyword, &vector, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn rrf_empty_inputs() {
        let keyword: Vec<(u32, &str)> = vec![];
        let vector: Vec<(u32, &str)> = vec![];

        let results = reciprocal_rank_fusion(&keyword, &vector, 10);
        assert!(results.is_empty());
    }
}
