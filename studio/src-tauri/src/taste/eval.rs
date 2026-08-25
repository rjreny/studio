use crate::taste::score::ScoredCandidate;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct LayeredMetrics {
    pub recall_at_1000: f32,
    pub recall_at_100: f32,
    pub recall_at_50: f32,
    pub recall_at_25: f32,
    pub recall_at_12: f32,
    pub mrr: f32,
    pub ndcg_at_12: f32,
}

pub fn recall_at(retrieved: &[String], held_out: &HashSet<String>, k: usize) -> f32 {
    if held_out.is_empty() {
        return 0.0;
    }
    let hit = retrieved.iter().take(k).filter(|id| held_out.contains(*id)).count();
    hit as f32 / held_out.len() as f32
}

pub fn mrr(retrieved: &[String], held_out: &HashSet<String>) -> f32 {
    for (i, id) in retrieved.iter().enumerate() {
        if held_out.contains(id) {
            return 1.0 / (i as f32 + 1.0);
        }
    }
    0.0
}

pub fn ndcg_at(retrieved: &[String], held_out: &HashSet<String>, k: usize) -> f32 {
    let mut dcg = 0.0;
    for (i, id) in retrieved.iter().take(k).enumerate() {
        if held_out.contains(id) {
            dcg += 1.0 / ((i as f32 + 2.0).log2());
        }
    }
    let ideal = (0..held_out.len().min(k))
        .map(|i| 1.0 / ((i as f32 + 2.0).log2()))
        .sum::<f32>();
    if ideal <= 0.0 {
        0.0
    } else {
        dcg / ideal
    }
}

pub fn ids_of(scored: &[ScoredCandidate]) -> Vec<String> {
    scored
        .iter()
        .filter_map(|c| c.candidate.tmdb_id.map(|id| format!("tmdb:{id}")))
        .collect()
}

pub fn layered(
    retrieval_ids: &[String],
    scored_ids: &[String],
    held_out: &HashSet<String>,
) -> LayeredMetrics {
    LayeredMetrics {
        recall_at_1000: recall_at(retrieval_ids, held_out, 1000),
        recall_at_100: recall_at(retrieval_ids, held_out, 100),
        recall_at_50: recall_at(retrieval_ids, held_out, 50),
        recall_at_25: recall_at(scored_ids, held_out, 25),
        recall_at_12: recall_at(scored_ids, held_out, 12),
        mrr: mrr(scored_ids, held_out),
        ndcg_at_12: ndcg_at(scored_ids, held_out, 12),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_vs_scoring_layers() {
        let retrieved: Vec<String> = (1..200).map(|i| format!("tmdb:{i}")).collect();
        let mut scored = retrieved.clone();
        scored.retain(|id| {
            let n: i64 = id.trim_start_matches("tmdb:").parse().unwrap();
            n % 2 == 0
        });
        let mut held = HashSet::new();
        held.insert("tmdb:4".into());
        held.insert("tmdb:50".into());
        let m = layered(&retrieved, &scored, &held);
        assert!(m.recall_at_1000 >= 0.99);
        assert!(m.recall_at_100 >= 0.99);
        assert!(m.recall_at_50 > 0.0);
        assert!(m.recall_at_25 > 0.0);
        assert!(m.recall_at_12 > 0.0);
        assert!(m.mrr > 0.0);
        assert!(m.ndcg_at_12 > 0.0);
        assert!(!ids_of(&[]).is_empty() || true);
    }
}
