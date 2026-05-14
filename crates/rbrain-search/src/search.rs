use std::collections::HashMap;

/// Reciprocal Rank Fusion (RRF) - combines multiple ranked result lists.
/// Each result list should contain `(chunk_id, rank)` tuples where rank starts at 1.
/// Returns fused results sorted by score descending.
pub fn rrf(result_lists: Vec<Vec<(i64, usize)>>, k: f64) -> Vec<(i64, f64)> {
    let mut scores: HashMap<i64, f64> = HashMap::new();

    for list in result_lists {
        for (rank_idx, (chunk_id, _)) in list.into_iter().enumerate() {
            let rank = rank_idx + 1; // 1-based rank
            *scores.entry(chunk_id).or_default() += 1.0 / (k + (rank as f64));
        }
    }

    let mut sorted: Vec<_> = scores.into_iter().collect();
    sorted.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}
