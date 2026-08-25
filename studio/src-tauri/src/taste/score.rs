use crate::taste::features::{
    decade_label, runtime_bucket, FeatureFamily, FeatureKey, FeatureProfile,
};
use crate::taste::retrieve::{Candidate, RetrievalKind};
use serde::{Deserialize, Serialize};

pub const W_CONTENT: f32 = 0.45;
pub const W_TMDB: f32 = 0.20;
pub const W_FRIEND: f32 = 0.15;
pub const W_RECENT: f32 = 0.10;
pub const W_WATCHLIST: f32 = 0.05;
pub const W_NOVELTY: f32 = 0.05;
pub const W_NEGATIVE: f32 = 0.35;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScore {
    pub content: f32,
    pub tmdb_related: f32,
    pub friend_affinity: f32,
    pub recent_taste: f32,
    pub watchlist: f32,
    pub novelty: f32,
    pub negative_evidence: f32,
    pub total: f32,
}

impl CandidateScore {
    pub fn clamp_components(&mut self) {
        self.content = self.content.clamp(-1.0, 1.0);
        self.tmdb_related = self.tmdb_related.clamp(0.0, 1.0);
        self.friend_affinity = self.friend_affinity.clamp(-1.0, 1.0);
        self.recent_taste = self.recent_taste.clamp(-1.0, 1.0);
        self.watchlist = self.watchlist.clamp(0.0, 1.0);
        self.novelty = self.novelty.clamp(-1.0, 1.0);
        self.negative_evidence = self.negative_evidence.clamp(-1.0, 0.0);
        self.total = (W_CONTENT * self.content
            + W_TMDB * self.tmdb_related
            + W_FRIEND * self.friend_affinity
            + W_RECENT * self.recent_taste
            + W_WATCHLIST * self.watchlist
            + W_NOVELTY * self.novelty
            + W_NEGATIVE * self.negative_evidence)
            .clamp(-1.5, 1.5);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredCandidate {
    pub candidate: CandidateView,
    pub score: CandidateScore,
    pub reasons: Vec<String>,
    pub evidence: Vec<String>,
    pub positive_features: Vec<String>,
    pub negative_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateView {
    pub tmdb_id: Option<i64>,
    pub title: String,
    pub year: Option<i32>,
    pub poster: Option<String>,
    pub watchlist: bool,
    pub sources: Vec<crate::taste::retrieve::RetrievalSource>,
    pub directors: Vec<String>,
    pub genres: Vec<String>,
}

pub fn score_candidate(profile: &FeatureProfile, candidate: &Candidate) -> ScoredCandidate {
    let mut content_sum = 0.0;
    let mut content_w = 0.0;
    let mut recent_sum = 0.0;
    let mut recent_w = 0.0;
    let mut neg = 0.0;
    let mut reasons = Vec::new();
    let mut evidence = Vec::new();
    let mut positive_features = Vec::new();
    let mut negative_features = Vec::new();

    let keys = candidate_keys(candidate);
    let mut family_used: std::collections::HashMap<FeatureFamily, usize> =
        std::collections::HashMap::new();

    for aff in &profile.affinities {
        if !keys.iter().any(|k| k.storage_key() == aff.key.storage_key()) {
            continue;
        }
        let used = family_used.entry(aff.key.family).or_insert(0);
        if *used >= aff.key.family.top_k() {
            continue;
        }
        *used += 1;
        let w = aff.key.family.weight();
        content_sum += aff.scoring_affinity();
        content_w += w;
        recent_sum += aff.recent_weight * aff.confidence * w;
        recent_w += w;
        if aff.negative_weight > aff.positive_weight * 0.6 && aff.negative_weight > 0.2 {
            neg += (aff.negative_weight / (aff.negative_weight + aff.positive_weight + 1e-4))
                * aff.confidence
                * w;
            negative_features.push(aff.key.name.clone());
            for e in aff.negative_evidence.iter().take(2) {
                evidence.push(e.title.clone());
            }
        }
        if aff.weighted_mean > 0.1 {
            positive_features.push(aff.key.name.clone());
            if reasons.len() < 4 {
                reasons.push(format!(
                    "{} affinity ({:.2})",
                    aff.key.name, aff.weighted_mean
                ));
            }
            for e in aff.positive_evidence.iter().take(2) {
                if evidence.len() < 6 {
                    evidence.push(e.title.clone());
                }
            }
        }
    }

    let content = if content_w > 0.0 {
        (content_sum / content_w).tanh()
    } else {
        0.0
    };
    let recent_taste = if recent_w > 0.0 {
        (recent_sum / recent_w).tanh()
    } else {
        0.0
    };
    let tmdb_related = if candidate.sources.iter().any(|s| s.kind == RetrievalKind::Related) {
        candidate.tmdb_related.clamp(0.4, 1.0)
    } else {
        0.0
    };
    let novelty = {
        let votes = candidate.vote_count.unwrap_or(0) as f32;
        if votes <= 0.0 {
            0.1
        } else {
            (1.0 - (votes / (votes + 800.0))).clamp(0.0, 1.0) * 2.0 - 1.0
        }
    };
    let mut score = CandidateScore {
        content,
        tmdb_related,
        friend_affinity: candidate.friend_affinity.clamp(-1.0, 1.0),
        recent_taste,
        watchlist: if candidate.watchlist { 1.0 } else { 0.0 },
        novelty,
        negative_evidence: (-neg).clamp(-1.0, 0.0),
        total: 0.0,
    };
    score.clamp_components();
    if candidate.watchlist {
        reasons.push("On your watchlist".into());
    }
    if score.friend_affinity > 0.15 {
        reasons.push("High-overlap friends rated this well".into());
    }
    evidence.sort();
    evidence.dedup();

    ScoredCandidate {
        candidate: CandidateView {
            tmdb_id: candidate.tmdb_id,
            title: candidate.title.clone(),
            year: candidate.year,
            poster: candidate.poster.clone(),
            watchlist: candidate.watchlist,
            sources: candidate.sources.clone(),
            directors: candidate
                .credits
                .iter()
                .filter(|c| c.job == "Director")
                .map(|c| c.name.clone())
                .collect(),
            genres: candidate.genres.clone(),
        },
        score,
        reasons,
        evidence,
        positive_features,
        negative_features,
    }
}

fn candidate_keys(candidate: &Candidate) -> Vec<FeatureKey> {
    let mut keys = Vec::new();
    for g in &candidate.genres {
        keys.push(FeatureKey::new(FeatureFamily::Genre, None, g));
    }
    for c in &candidate.credits {
        let family = match c.job.as_str() {
            "Director" => FeatureFamily::Director,
            "Writer" | "Screenplay" | "Original Screenplay" | "Story" => FeatureFamily::Writer,
            "Director of Photography" | "Cinematography" => FeatureFamily::Cinematographer,
            "Original Music Composer" | "Music" => FeatureFamily::Composer,
            "Actor" => FeatureFamily::Actor,
            _ => continue,
        };
        keys.push(FeatureKey::new(family, c.id, &c.name));
    }
    for k in &candidate.keywords {
        keys.push(FeatureKey::new(FeatureFamily::Keyword, k.id, &k.name));
    }
    if let Some(y) = candidate.year {
        keys.push(FeatureKey::new(FeatureFamily::Decade, None, decade_label(y)));
    }
    if let Some(rt) = candidate.runtime {
        keys.push(FeatureKey::new(
            FeatureFamily::Runtime,
            None,
            runtime_bucket(rt),
        ));
    }
    keys
}

pub fn score_all(profile: &FeatureProfile, candidates: &[Candidate]) -> Vec<ScoredCandidate> {
    let mut scored: Vec<_> = candidates
        .iter()
        .map(|c| score_candidate(profile, c))
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .total
            .partial_cmp(&a.score.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(100);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_stay_in_range() {
        let mut s = CandidateScore {
            content: 4.0,
            tmdb_related: 3.0,
            friend_affinity: -4.0,
            recent_taste: 2.0,
            watchlist: 5.0,
            novelty: 9.0,
            negative_evidence: -4.0,
            total: 0.0,
        };
        s.clamp_components();
        assert!((-1.0..=1.0).contains(&s.content));
        assert!((0.0..=1.0).contains(&s.tmdb_related));
        assert!((-1.0..=1.0).contains(&s.friend_affinity));
        assert!((-1.0..=1.0).contains(&s.recent_taste));
        assert!((0.0..=1.0).contains(&s.watchlist));
        assert!((-1.0..=1.0).contains(&s.novelty));
        assert!((-1.0..=0.0).contains(&s.negative_evidence));
    }
}
