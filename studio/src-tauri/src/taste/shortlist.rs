use crate::taste::score::ScoredCandidate;

const MMR_LAMBDA: f32 = 0.72;
const SHORTLIST_MIN: usize = 30;
const SHORTLIST_MAX: usize = 50;

fn similarity(a: &ScoredCandidate, b: &ScoredCandidate) -> f32 {
    let mut s = 0.0;
    if !a.candidate.directors.is_empty()
        && a.candidate
            .directors
            .iter()
            .any(|d| b.candidate.directors.contains(d))
    {
        s += 0.6;
    }
    if !a.candidate.genres.is_empty() {
        let shared = a
            .candidate
            .genres
            .iter()
            .filter(|g| b.candidate.genres.contains(g))
            .count();
        if shared > 0 {
            s += 0.25 * (shared as f32).min(2.0);
        }
    }
    let same_seed = a.candidate.sources.iter().any(|sa| {
        b.candidate.sources.iter().any(|sb| {
            sa.kind == sb.kind && sa.seed_tmdb_id.is_some() && sa.seed_tmdb_id == sb.seed_tmdb_id
        })
    });
    if same_seed {
        s += 0.2;
    }
    if !a.candidate.modes.is_empty() {
        let shared = a
            .candidate
            .modes
            .iter()
            .filter(|m| b.candidate.modes.contains(m))
            .count();
        if shared > 0 {
            s += 0.35;
        }
    }
    s.min(1.0)
}

/// Light MMR: keep cluster-mates. This is not the final 12.
pub fn shortlist(ranked: &[ScoredCandidate]) -> Vec<ScoredCandidate> {
    if ranked.is_empty() {
        return Vec::new();
    }
    let pool: Vec<&ScoredCandidate> = ranked.iter().take(100).collect();
    let mut selected: Vec<ScoredCandidate> = Vec::new();
    let mut remaining: Vec<usize> = (0..pool.len()).collect();
    let target = SHORTLIST_MAX.min(pool.len()).max(SHORTLIST_MIN.min(pool.len()));

    while selected.len() < target && !remaining.is_empty() {
        let mut best_i = 0;
        let mut best_score = f32::NEG_INFINITY;
        for (idx, &pi) in remaining.iter().enumerate() {
            let rel = (pool[pi].score.total + 1.5) / 3.0;
            let sim = selected
                .iter()
                .map(|s| similarity(pool[pi], s))
                .fold(0.0_f32, f32::max);
            let mmr = MMR_LAMBDA * rel - (1.0 - MMR_LAMBDA) * sim;
            if mmr > best_score {
                best_score = mmr;
                best_i = idx;
            }
        }
        let pi = remaining.remove(best_i);
        selected.push(pool[pi].clone());
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::retrieve::{RetrievalKind, RetrievalSource};
    use crate::taste::score::{CandidateScore, CandidateView};

    fn cand(title: &str, director: &str, total: f32) -> ScoredCandidate {
        ScoredCandidate {
            candidate: CandidateView {
                tmdb_id: Some(title.len() as i64),
                title: title.into(),
                year: Some(1995),
                poster: None,
                watchlist: false,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Related,
                    label: "x".into(),
                    seed_tmdb_id: Some(1),
                }],
                directors: vec![director.into()],
                genres: vec!["Crime".into()],
                modes: vec![],
            },
            score: CandidateScore {
                content: total,
                tmdb_related: 0.5,
                friend_affinity: 0.0,
                recent_taste: 0.0,
                watchlist: 0.0,
                novelty: 0.0,
                negative_evidence: 0.0,
                total,
            },
            reasons: vec!["director".into()],
            evidence: vec!["Heat".into()],
            positive_features: vec![director.into()],
            negative_features: vec![],
            contextual_only: false,
        }
    }

    #[test]
    fn shortlist_keeps_more_than_twelve() {
        let ranked: Vec<_> = (0..80)
            .map(|i| cand(&format!("Film {i}"), if i < 40 { "Mann" } else { "Other" }, 0.9 - i as f32 * 0.005))
            .collect();
        let s = shortlist(&ranked);
        assert!(s.len() >= 30);
        assert!(s.len() <= 50);
        let mann = s.iter().filter(|c| c.candidate.directors.iter().any(|d| d == "Mann")).count();
        assert!(mann >= 2, "light MMR must not wipe a dense cluster");
    }
}
