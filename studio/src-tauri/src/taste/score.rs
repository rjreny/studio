use crate::taste::features::{
    decade_label, family_for_job, runtime_bucket, FeatureFamily, FeatureKey, FeatureProfile,
    PORTABILITY_SKIP,
};
use crate::taste::dimensions::predicted_modes;
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
    #[serde(default)]
    pub contextual_only: bool,
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
    #[serde(default)]
    pub modes: Vec<String>,
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
    let matched_primary = profile.affinities.iter().any(|aff| {
        aff.portability >= PORTABILITY_SKIP
            && aff.key.family.is_primary()
            && keys.iter().any(|k| k.storage_key() == aff.key.storage_key())
    });
    let mut family_used: std::collections::HashMap<FeatureFamily, usize> =
        std::collections::HashMap::new();

    for aff in &profile.affinities {
        if aff.portability < PORTABILITY_SKIP {
            continue;
        }
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
        recent_sum += aff.recent_weight * aff.confidence * w * aff.portability;
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
        if aff.recommendation_mean > 0.1 {
            positive_features.push(aff.key.name.clone());
            let cite_contextual = aff.key.family.is_contextual() && matched_primary;
            if reasons.len() < 4 && !cite_contextual {
                reasons.push(format!(
                    "{} affinity ({:.2})",
                    aff.key.name, aff.recommendation_mean
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
            modes: predicted_modes(&candidate.genres, &candidate.credits, &candidate.keywords),
        },
        score,
        reasons,
        evidence,
        positive_features,
        negative_features,
        contextual_only: !matched_primary,
    }
}

fn candidate_keys(candidate: &Candidate) -> Vec<FeatureKey> {
    let mut keys = Vec::new();
    for g in &candidate.genres {
        keys.push(FeatureKey::new(FeatureFamily::Genre, None, g));
    }
    for c in &candidate.credits {
        let Some(family) = family_for_job(&c.job) else {
            continue;
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

    #[test]
    fn cinematographer_outranks_decade_only() {
        use crate::taste::features::{
            build_profile, observations_from_film, Credit, Keyword,
        };
        use crate::taste::preference::{interaction_signal, rating_profile};
        use crate::taste::retrieve::{Candidate, RetrievalKind, RetrievalSource};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let dp = Credit {
            id: Some(77),
            name: "Greig Fraser".into(),
            job: "Director of Photography".into(),
        };
        let mut obs = observations_from_film(
            "The Batman",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into()],
            &[dp.clone()],
            &[],
            Some(2022),
            Some(176),
        );
        obs.extend(observations_from_film(
            "Project Hail Mary",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.2), 1, false),
            Some(0.2),
            &["Science Fiction".into()],
            &[dp.clone()],
            &[],
            Some(2026),
            Some(140),
        ));
        for i in 0..20 {
            let s = interaction_signal(4.5, &p, Some(8.0), 6, false);
            obs.extend(observations_from_film(
                &format!("kid{i}"),
                4.5,
                Some(10 + i),
                &s,
                Some(8.0),
                &["Comedy".into()],
                &[],
                &[],
                Some(2005),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let dp_cand = Candidate {
            tmdb_id: Some(99),
            title: "Dune".into(),
            year: Some(2021),
            poster: None,
            genres: vec!["Science Fiction".into()],
            credits: vec![dp],
            keywords: Vec::<Keyword>::new(),
            runtime: Some(155),
            vote_count: Some(1000),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Filmography,
                label: "Greig Fraser".into(),
                seed_tmdb_id: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        };
        let decade_cand = Candidate {
            tmdb_id: Some(100),
            title: "Tinker Bell".into(),
            year: Some(2008),
            poster: None,
            genres: vec!["Animation".into()],
            credits: vec![],
            keywords: vec![],
            runtime: Some(78),
            vote_count: Some(100),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar to childhood".into(),
                seed_tmdb_id: Some(10),
            }],
            friend_affinity: 0.0,
            tmdb_related: 1.0,
        };
        let dp_score = score_candidate(&profile, &dp_cand);
        let decade_score = score_candidate(&profile, &decade_cand);
        assert!(
            dp_score.score.content > decade_score.score.content,
            "dp {} decade {}",
            dp_score.score.content,
            decade_score.score.content
        );
        assert!(!dp_score.contextual_only);
    }
}
