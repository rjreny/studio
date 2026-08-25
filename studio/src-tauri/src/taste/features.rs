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
const CINEMATOGRAPHER_K: f32 = 2.5;
pub const RECENT_YEARS: f32 = 1.5;
const SHIFT_RECENT_N: usize = 8;
const SHIFT_LONG_N: usize = 20;
const SHIFT_CONF: f32 = 0.4;
const SHIFT_DELTA: f32 = 0.35;
const POLARIZING_VAR: f32 = 0.12;
pub const PORTABILITY_SKIP: f32 = 0.15;
pub const PORTABLE_CONTEXTUAL: f32 = 0.4;

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

    pub fn is_contextual(self) -> bool {
        matches!(self, Self::Decade | Self::Runtime)
    }

    pub fn is_primary(self) -> bool {
        !self.is_contextual()
    }

    fn k(self) -> f32 {
        match self {
            Self::Cinematographer => CINEMATOGRAPHER_K,
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

pub fn family_for_job(job: &str) -> Option<FeatureFamily> {
    match job {
        "Director" => Some(FeatureFamily::Director),
        "Writer" | "Screenplay" | "Original Screenplay" | "Story" => Some(FeatureFamily::Writer),
        "Director of Photography" | "Cinematography" | "Cinematographer" => {
            Some(FeatureFamily::Cinematographer)
        }
        "Original Music Composer" | "Music" => Some(FeatureFamily::Composer),
        "Actor" => Some(FeatureFamily::Actor),
        _ => None,
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
    pub preference_mean: f32,
    pub recommendation_mean: f32,
    pub weighted_variance: f32,
    pub positive_weight: f32,
    pub negative_weight: f32,
    pub recent_weight: f32,
    pub long_term_weight: f32,
    pub confidence: f32,
    pub feature_strength: f32,
    pub portability: f32,
    pub positive_evidence: Vec<EvidenceFilm>,
    pub negative_evidence: Vec<EvidenceFilm>,
}

impl FeatureAffinity {
    pub fn scoring_affinity(&self) -> f32 {
        self.recommendation_mean
            * self.confidence
            * self.key.family.weight()
            * self.portability
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureProfile {
    pub affinities: Vec<FeatureAffinity>,
    pub polarizing: Vec<PolarizingFeature>,
    pub shifts: Vec<RecentShift>,
    #[serde(default)]
    pub dimensions: Vec<crate::taste::dimensions::TasteDimensionView>,
    #[serde(default)]
    pub modes: Vec<crate::taste::dimensions::TasteMode>,
    #[serde(default)]
    pub mode_shifts: Vec<crate::taste::dimensions::ModeShift>,
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
    pub preference_weight: f32,
    pub recommendation_weight: f32,
    pub discovery_strength: f32,
    pub age_years: Option<f32>,
    pub evidence: EvidenceFilm,
    pub positive: f32,
    pub negative: f32,
    pub genres: Vec<String>,
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
        let pairs_pref: Vec<(f32, f32)> = group
            .iter()
            .map(|o| (o.affinity_preference, o.preference_weight))
            .collect();
        let pairs_rec: Vec<(f32, f32)> = group
            .iter()
            .map(|o| (o.affinity_preference, o.recommendation_weight))
            .collect();
        let Some(preference_mean) = weighted_mean(&pairs_pref) else {
            continue;
        };
        let recommendation_mean = weighted_mean(&pairs_rec).unwrap_or(preference_mean);
        let var = weighted_variance(&pairs_pref, preference_mean);
        let appearances = group.len() as u32;
        let confidence = 1.0 - (-(appearances as f32) / key.family.k()).exp();
        let portability = feature_portability(&key, group);
        let mut positive_weight = 0.0;
        let mut negative_weight = 0.0;
        let mut recent_pairs = Vec::new();
        let mut long_pairs = Vec::new();
        let mut positive_evidence = Vec::new();
        let mut negative_evidence = Vec::new();
        for o in group {
            positive_weight += o.positive * o.preference_weight;
            negative_weight += o.negative * o.preference_weight;
            let recent = o.age_years.map(|y| y <= RECENT_YEARS).unwrap_or(false);
            if recent {
                recent_pairs.push((o.affinity_preference, o.recommendation_weight));
            } else {
                long_pairs.push((o.affinity_preference, o.recommendation_weight));
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
            weighted_mean: preference_mean,
            preference_mean,
            recommendation_mean,
            weighted_variance: var,
            positive_weight,
            negative_weight,
            recent_weight: weighted_mean(&recent_pairs).unwrap_or(0.0),
            long_term_weight: weighted_mean(&long_pairs).unwrap_or(preference_mean),
            confidence,
            feature_strength: recommendation_mean.abs(),
            portability,
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
        dimensions: Vec::new(),
        modes: Vec::new(),
        mode_shifts: Vec::new(),
    }
}

fn feature_portability(key: &FeatureKey, group: &[&FeatureObservation]) -> f32 {
    if !key.family.is_contextual() {
        return 1.0;
    }
    let mut genres = std::collections::HashSet::new();
    let mut disc = 0.0;
    let mut n = 0.0;
    for o in group {
        if o.positive > 0.0 {
            for g in &o.genres {
                genres.insert(g.to_lowercase());
            }
        }
        disc += o.discovery_strength;
        n += 1.0;
    }
    let unique = genres.len();
    if unique < 3 {
        return 0.0;
    }
    let genre_port = ((unique as f32 - 2.0) / 4.0).clamp(0.0, 1.0);
    let mean_disc = if n > 0.0 { disc / n } else { 1.0 };
    genre_port * mean_disc
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
            preference_weight: signal.preference_weight,
            recommendation_weight: signal.recommendation_weight,
            discovery_strength: signal.discovery_strength,
            age_years,
            evidence: evidence.clone(),
            positive,
            negative,
            genres: genres.to_vec(),
        });
    };
    for g in genres {
        push(&mut out, FeatureKey::new(FeatureFamily::Genre, None, g));
    }
    for c in credits {
        let Some(family) = family_for_job(&c.job) else {
            continue;
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
        let expected = (s5.preference.affinity_preference * s5.preference_weight
            + s4.preference.affinity_preference * s4.preference_weight)
            / (s5.preference_weight + s4.preference_weight);
        assert!((drama.preference_mean - expected).abs() < 1e-4);
        assert!((drama.weighted_mean - drama.preference_mean).abs() < 1e-6);
        assert!((drama.portability - 1.0).abs() < 1e-5);
    }

    #[test]
    fn nostalgia_decade_is_not_portable() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let mut obs = Vec::new();
        for i in 0..40 {
            let s = interaction_signal(4.5, &p, Some(8.0), 6, false);
            obs.extend(observations_from_film(
                &format!("kid{i}"),
                4.5,
                Some(i),
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
        let decade = profile
            .affinities
            .iter()
            .find(|a| a.key.family == FeatureFamily::Decade)
            .unwrap();
        assert_eq!(decade.portability, 0.0);
        assert!(decade.scoring_affinity().abs() < 1e-5);
        assert!(decade.preference_mean > 0.0);
    }

    #[test]
    fn diverse_first_watch_decade_is_portable() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let genres = ["Drama", "Thriller", "Action", "Horror", "Comedy"];
        let mut obs = Vec::new();
        for (i, g) in genres.iter().cycle().take(20).enumerate() {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("new{i}"),
                4.5,
                Some(i as i64),
                &s,
                Some(0.4),
                &[(*g).into()],
                &[],
                &[],
                Some(2005),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let decade = profile
            .affinities
            .iter()
            .find(|a| a.key.family == FeatureFamily::Decade)
            .unwrap();
        assert!(decade.portability >= 0.4, "got {}", decade.portability);
        assert!(decade.scoring_affinity() > 0.0);
    }

    #[test]
    fn two_films_establish_cinematographer() {
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
            &[dp],
            &[],
            Some(2026),
            Some(140),
        ));
        let profile = build_profile(&obs);
        let fraser = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .unwrap();
        assert_eq!(fraser.positive_evidence.len(), 2);
        assert!(fraser.confidence > 0.4, "got {}", fraser.confidence);
        assert!(fraser.scoring_affinity() > 0.0);
        assert!((fraser.portability - 1.0).abs() < 1e-5);
    }
}
