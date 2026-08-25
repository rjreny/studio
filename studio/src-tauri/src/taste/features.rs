use crate::taste::preference::{
    weighted_mean, weighted_variance, InteractionSignal,
};
use serde::{Deserialize, Serialize};

pub const DIRECTOR_W: f32 = 1.00;
pub const WRITER_W: f32 = 0.85;
pub const CINEMATOGRAPHER_W: f32 = 0.80;
pub const COMPOSER_W: f32 = 0.65;
pub const ACTOR_W: f32 = 0.55;
pub const KEYWORD_W: f32 = 0.45;
pub const GENRE_W: f32 = 0.35;
pub const DECADE_W: f32 = 0.20;
pub const RUNTIME_W: f32 = 0.15;

const PEOPLE_K: f32 = 4.0;
const BROAD_K: f32 = 6.0;
const RECENT_YEARS: f32 = 1.5;
const SHIFT_RECENT_N: usize = 8;
const SHIFT_LONG_N: usize = 20;
const SHIFT_CONF: f32 = 0.4;
const SHIFT_DELTA: f32 = 0.35;
const POLARIZING_VAR: f32 = 0.12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatureFamily {
    Director,
    Writer,
    Cinematographer,
    Composer,
    Actor,
    Genre,
    Keyword,
    Decade,
    Runtime,
}

impl FeatureFamily {
    pub fn weight(self) -> f32 {
        match self {
            Self::Director => DIRECTOR_W,
            Self::Writer => WRITER_W,
            Self::Cinematographer => CINEMATOGRAPHER_W,
            Self::Composer => COMPOSER_W,
            Self::Actor => ACTOR_W,
            Self::Keyword => KEYWORD_W,
            Self::Genre => GENRE_W,
            Self::Decade => DECADE_W,
            Self::Runtime => RUNTIME_W,
        }
    }

    fn k(self) -> f32 {
        match self {
            Self::Genre | Self::Decade | Self::Runtime => BROAD_K,
            _ => PEOPLE_K,
        }
    }

    pub fn top_k(self) -> usize {
        match self {
            Self::Director | Self::Writer | Self::Cinematographer | Self::Composer => 2,
            Self::Actor => 3,
            Self::Keyword => 5,
            _ => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureKey {
    pub family: FeatureFamily,
    pub id: Option<i64>,
    pub name: String,
}

impl FeatureKey {
    pub fn new(family: FeatureFamily, id: Option<i64>, name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            family,
            id,
            name: name.trim().to_string(),
        }
    }

    pub fn storage_key(&self) -> String {
        match self.id {
            Some(id) => format!("{:?}:{id}", self.family),
            None => format!("{:?}:{}", self.family, self.name.to_lowercase()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceFilm {
    pub title: String,
    pub rating: f32,
    pub tmdb_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureAffinity {
    pub key: FeatureKey,
    pub appearances: u32,
    pub weighted_mean: f32,
    pub weighted_variance: f32,
    pub positive_weight: f32,
    pub negative_weight: f32,
    pub recent_weight: f32,
    pub long_term_weight: f32,
    pub confidence: f32,
    pub positive_evidence: Vec<EvidenceFilm>,
    pub negative_evidence: Vec<EvidenceFilm>,
}

impl FeatureAffinity {
    pub fn scoring_affinity(&self) -> f32 {
        self.weighted_mean * self.confidence * self.key.family.weight()
    }

    pub fn polarizing(&self) -> bool {
        self.weighted_variance >= POLARIZING_VAR
            && !self.positive_evidence.is_empty()
            && !self.negative_evidence.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolarizingFeature {
    pub feature: String,
    pub family: FeatureFamily,
    pub id: Option<i64>,
    pub confidence: f32,
    pub affinity: f32,
    pub variance: f32,
    pub positive_evidence: Vec<EvidenceFilm>,
    pub negative_evidence: Vec<EvidenceFilm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentShift {
    pub feature: String,
    pub family: FeatureFamily,
    pub long_term: f32,
    pub recent: f32,
    pub delta: f32,
    pub long_term_evidence: Vec<EvidenceFilm>,
    pub recent_evidence: Vec<EvidenceFilm>,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureProfile {
    pub affinities: Vec<FeatureAffinity>,
    pub polarizing: Vec<PolarizingFeature>,
    pub shifts: Vec<RecentShift>,
}

#[derive(Debug, Clone)]
pub struct Credit {
    pub id: Option<i64>,
    pub name: String,
    pub job: String,
}

#[derive(Debug, Clone)]
pub struct Keyword {
    pub id: Option<i64>,
    pub name: String,
}

pub fn runtime_bucket(rt: i32) -> &'static str {
    if rt < 90 {
        "under 90"
    } else if rt < 120 {
        "90-119"
    } else if rt < 150 {
        "120-149"
    } else {
        "150+"
    }
}

pub fn decade_label(year: i32) -> String {
    format!("{}s", (year / 10) * 10)
}

pub struct FeatureObservation {
    pub key: FeatureKey,
    pub affinity_preference: f32,
    pub effective_weight: f32,
    pub age_years: Option<f32>,
    pub evidence: EvidenceFilm,
    pub positive: f32,
    pub negative: f32,
}

pub fn build_profile(obs: &[FeatureObservation]) -> FeatureProfile {
    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<&FeatureObservation>> = HashMap::new();
    for o in obs {
        groups.entry(o.key.storage_key()).or_default().push(o);
    }
    let mut affinities = Vec::new();
    for group in groups.values() {
        let key = group[0].key.clone();
        let pairs: Vec<(f32, f32)> = group
            .iter()
            .map(|o| (o.affinity_preference, o.effective_weight))
            .collect();
        let Some(mean) = weighted_mean(&pairs) else {
            continue;
        };
        let var = weighted_variance(&pairs, mean);
        let appearances = group.len() as u32;
        let confidence = 1.0 - (-(appearances as f32) / key.family.k()).exp();
        let mut positive_weight = 0.0;
        let mut negative_weight = 0.0;
        let mut recent_pairs = Vec::new();
        let mut long_pairs = Vec::new();
        let mut positive_evidence = Vec::new();
        let mut negative_evidence = Vec::new();
        for o in group {
            positive_weight += o.positive * o.effective_weight;
            negative_weight += o.negative * o.effective_weight;
            let recent = o.age_years.map(|y| y <= RECENT_YEARS).unwrap_or(false);
            if recent {
                recent_pairs.push((o.affinity_preference, o.effective_weight));
            } else {
                long_pairs.push((o.affinity_preference, o.effective_weight));
            }
            if o.positive > 0.0 && positive_evidence.len() < 6 {
                positive_evidence.push(o.evidence.clone());
            }
            if o.negative > 0.0 && negative_evidence.len() < 6 {
                negative_evidence.push(o.evidence.clone());
            }
        }
        affinities.push(FeatureAffinity {
            key,
            appearances,
            weighted_mean: mean,
            weighted_variance: var,
            positive_weight,
            negative_weight,
            recent_weight: weighted_mean(&recent_pairs).unwrap_or(0.0),
            long_term_weight: weighted_mean(&long_pairs).unwrap_or(mean),
            confidence,
            positive_evidence,
            negative_evidence,
        });
    }
    affinities.sort_by(|a, b| {
        b.scoring_affinity()
            .partial_cmp(&a.scoring_affinity())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let polarizing: Vec<PolarizingFeature> = affinities
        .iter()
        .filter(|a| a.polarizing())
        .take(12)
        .map(|a| PolarizingFeature {
            feature: a.key.name.clone(),
            family: a.key.family,
            id: a.key.id,
            confidence: a.confidence,
            affinity: a.weighted_mean,
            variance: a.weighted_variance,
            positive_evidence: a.positive_evidence.clone(),
            negative_evidence: a.negative_evidence.clone(),
        })
        .collect();

    let recent_n = obs
        .iter()
        .filter(|o| o.age_years.map(|y| y <= RECENT_YEARS).unwrap_or(false))
        .count();
    let long_n = obs.len().saturating_sub(recent_n);
    let mut shifts = Vec::new();
    if recent_n >= SHIFT_RECENT_N && long_n >= SHIFT_LONG_N {
        for a in &affinities {
            if a.confidence < SHIFT_CONF {
                continue;
            }
            let delta = a.recent_weight - a.long_term_weight;
            if delta.abs() < SHIFT_DELTA {
                continue;
            }
            let recent_ev: Vec<EvidenceFilm> = obs
                .iter()
                .filter(|o| {
                    o.key.storage_key() == a.key.storage_key()
                        && o.age_years.map(|y| y <= RECENT_YEARS).unwrap_or(false)
                })
                .map(|o| o.evidence.clone())
                .take(6)
                .collect();
            if recent_ev.len() < 3 {
                continue;
            }
            shifts.push(RecentShift {
                feature: a.key.name.clone(),
                family: a.key.family,
                long_term: a.long_term_weight,
                recent: a.recent_weight,
                delta,
                long_term_evidence: a.positive_evidence.iter().chain(a.negative_evidence.iter()).cloned().take(6).collect(),
                recent_evidence: recent_ev,
            });
        }
    }
    shifts.sort_by(|a, b| {
        b.delta
            .abs()
            .partial_cmp(&a.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    shifts.truncate(8);

    FeatureProfile {
        affinities,
        polarizing,
        shifts,
    }
}

pub fn observations_from_film(
    title: &str,
    rating: f32,
    tmdb_id: Option<i64>,
    signal: &InteractionSignal,
    age_years: Option<f32>,
    genres: &[String],
    credits: &[Credit],
    keywords: &[Keyword],
    year: Option<i32>,
    runtime: Option<i32>,
) -> Vec<FeatureObservation> {
    let evidence = EvidenceFilm {
        title: title.to_string(),
        rating,
        tmdb_id,
    };
    let abs = signal.preference.absolute;
    let positive = abs.max(0.0) * crate::taste::preference::relative_strength(signal.preference.relative);
    let negative = (-abs).max(0.0);
    let mut out = Vec::new();
    let push = |out: &mut Vec<FeatureObservation>, key: FeatureKey| {
        out.push(FeatureObservation {
            key,
            affinity_preference: signal.preference.affinity_preference,
            effective_weight: signal.effective_weight,
            age_years,
            evidence: evidence.clone(),
            positive,
            negative,
        });
    };
    for g in genres {
        push(&mut out, FeatureKey::new(FeatureFamily::Genre, None, g));
    }
    for c in credits {
        let family = match c.job.as_str() {
            "Director" => FeatureFamily::Director,
            "Writer" | "Screenplay" | "Original Screenplay" | "Story" => FeatureFamily::Writer,
            "Director of Photography" | "Cinematography" => FeatureFamily::Cinematographer,
            "Original Music Composer" | "Music" => FeatureFamily::Composer,
            "Actor" => FeatureFamily::Actor,
            _ => continue,
        };
        push(&mut out, FeatureKey::new(family, c.id, &c.name));
    }
    for k in keywords {
        push(&mut out, FeatureKey::new(FeatureFamily::Keyword, k.id, &k.name));
    }
    if let Some(y) = year {
        push(&mut out, FeatureKey::new(FeatureFamily::Decade, None, decade_label(y)));
    }
    if let Some(rt) = runtime {
        push(
            &mut out,
            FeatureKey::new(FeatureFamily::Runtime, None, runtime_bucket(rt)),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::preference::{interaction_signal, rating_profile};

    fn sig(rating: f32) -> InteractionSignal {
        let p = rating_profile(&[5.0, 4.5, 4.0, 3.5, 3.0, 2.5, 4.0, 4.5]).unwrap();
        interaction_signal(rating, &p, Some(1.0), 1, false)
    }

    #[test]
    fn polarizing_needs_both_sides() {
        let heat = observations_from_film(
            "Heat",
            5.0,
            Some(1),
            &sig(5.0),
            Some(3.0),
            &["Crime".into()],
            &[Credit {
                id: Some(10),
                name: "Michael Mann".into(),
                job: "Director".into(),
            }],
            &[],
            Some(1995),
            Some(170),
        );
        let blackhat = observations_from_film(
            "Blackhat",
            2.0,
            Some(2),
            &sig(2.0),
            Some(4.0),
            &["Crime".into()],
            &[Credit {
                id: Some(10),
                name: "Michael Mann".into(),
                job: "Director".into(),
            }],
            &[],
            Some(2015),
            Some(133),
        );
        let mut all = heat;
        all.extend(blackhat);
        // repeat to raise variance confidence
        let extra = observations_from_film(
            "Collateral",
            4.5,
            Some(3),
            &sig(4.5),
            Some(2.0),
            &["Crime".into()],
            &[Credit {
                id: Some(10),
                name: "Michael Mann".into(),
                job: "Director".into(),
            }],
            &[],
            Some(2004),
            Some(120),
        );
        all.extend(extra);
        let profile = build_profile(&all);
        let mann = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Michael Mann")
            .unwrap();
        assert!(!mann.positive_evidence.is_empty());
        assert!(!mann.negative_evidence.is_empty());
    }

    #[test]
    fn recent_shift_is_gated() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let mut obs = Vec::new();
        for i in 0..25 {
            let s = interaction_signal(2.5, &p, Some(4.0), 1, false);
            obs.extend(observations_from_film(
                &format!("old{i}"),
                2.5,
                Some(i),
                &s,
                Some(4.0),
                &["Animation".into()],
                &[],
                &[],
                Some(2000),
                None,
            ));
        }
        let profile = build_profile(&obs);
        assert!(profile.shifts.is_empty());
    }

    #[test]
    fn weighted_mean_uses_affinity_preference() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let s5 = interaction_signal(5.0, &p, Some(1.0), 1, false);
        let s4 = interaction_signal(4.0, &p, Some(1.0), 1, false);
        let mut obs = observations_from_film(
            "A",
            5.0,
            Some(1),
            &s5,
            Some(1.0),
            &["Drama".into()],
            &[],
            &[],
            None,
            None,
        );
        obs.extend(observations_from_film(
            "B",
            4.0,
            Some(2),
            &s4,
            Some(1.0),
            &["Drama".into()],
            &[],
            &[],
            None,
            None,
        ));
        let profile = build_profile(&obs);
        let drama = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Drama")
            .unwrap();
        let expected = (s5.preference.affinity_preference * s5.effective_weight
            + s4.preference.affinity_preference * s4.effective_weight)
            / (s5.effective_weight + s4.effective_weight);
        assert!((drama.weighted_mean - expected).abs() < 1e-4);
    }
}
