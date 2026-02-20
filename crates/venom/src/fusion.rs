use std::collections::HashMap;

use slither_core::SearchResult;

const RRF_K: usize = 60;

pub fn reciprocal_rank_fusion(lists: &[Vec<SearchResult>], limit: usize) -> Vec<SearchResult> {
    let mut scores: HashMap<u64, f64> = HashMap::new();
    let mut doc_map: HashMap<u64, SearchResult> = HashMap::new();

    for list in lists {
        for (rank, result) in list.iter().enumerate() {
            let score = 1.0 / (RRF_K + rank + 1) as f64;
            *scores.entry(result.doc_id).or_insert(0.0) += score;
            doc_map
                .entry(result.doc_id)
                .or_insert_with(|| result.clone());
        }
    }

    let mut fused: Vec<SearchResult> = scores
        .into_iter()
        .filter_map(|(doc_id, score)| {
            doc_map.remove(&doc_id).map(|mut r| {
                r.score = score as f32;
                r
            })
        })
        .collect();

    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    fused.truncate(limit);
    fused
}
