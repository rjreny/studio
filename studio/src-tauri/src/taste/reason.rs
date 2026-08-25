use crate::taste::features::FeatureProfile;
use crate::taste::retrieve::FilmRecord;
use crate::taste::score::ScoredCandidate;
use serde::Deserialize;
use serde_json::{json, Value};

pub const CALL1_SYSTEM: &str = r#"You are a film-taste critic inside Studio.
The deterministic system already scored a shortlist from the user's complete rating history.
You do NOT select recommendations. You critique the shortlist and optionally request targeted research.

Return JSON only:
{
  "candidateAssessments": [
    {"id":"tmdb:123","fit":"strong|mixed|superficial","reason":"...","concerns":["..."]}
  ],
  "tasteGaps": [
    {"facet":"...","missingFromShortlist":true}
  ],
  "discoveryQueries": [
    {"query":"...","targetFacet":"...","why":"..."}
  ]
}

Rules:
- Assess candidates that look superficial, polarizing, or mismatched. You may skip obvious strong fits.
- fit must be one of: strong, mixed, superficial.
- Mark contextualOnly candidates as fit=superficial unless another primary feature clearly saves them.
- Decade, runtime, and catalog exposure are not recommendation targets.
- At most 3 discoveryQueries. Each must name a specific facet the shortlist is missing, preferably a taste mode (visual, comedy, intensity, spectacle, atmosphere, comfort).
- Do not emit picks, rankings, or a recommended list.
- Do not ignore negative evidence or polarizing features.
- Raw JSON object only.
"#;

pub const CALL2_SYSTEM: &str = r#"You are the final film recommendation reasoner inside Studio.
You receive a scored shortlist, critic assessments, and optionally a few validated discoveries that already passed TMDB identity + scoring.

Return JSON only:
{
  "title": "short taste type, 2-5 words",
  "summary": "2-3 sentences on WHY they like and dislike. Cite specific films.",
  "affinities": [{"label":"...","evidence":"..."}],
  "aversions": [{"label":"...","evidence":"..."}],
  "dimensions": [
    {"name":"visual","take":"..."},
    {"name":"story","take":"..."},
    {"name":"intensity","take":"..."},
    {"name":"comedy","take":"..."},
    {"name":"spectacle","take":"..."},
    {"name":"atmosphere","take":"..."},
    {"name":"comfort","take":"..."}
  ],
  "picks": [
    {"id":"tmdb:123","title":"...","year":1999,"why":"...","mode":"core|deepCut|adjacent|discovery","rhymesWith":["..."]}
  ]
}

Selection contract:
- Exactly 12 picks.
- At least 8 from the original shortlist (use their id).
- At most 3 discoveries. Discovery is optional.
- Favor distinct high-confidence taste modes (visual, comedy, intensity, spectacle, atmosphere, comfort). Mix follows mode strengths, not one personality and not fixed quotas.
- Never pick a contextualOnly candidate. Never label one core.
- Never pick a film whose only reason is a decade or runtime. Catalog exposure is not a hunt target.
- Affinity evidence must be films that actually carry that person or feature. Do not attach unrelated titles from other affinities.
- Do not hunt for a decade or treat catalog exposure as taste. Older films are allowed when they have portable evidence (people, visual language, keywords).
- If the visual dimension is strong, cinematography is a real factor. Do not claim it is irrelevant.
- Do not invent titles. Every pick id must appear in the shortlist or validatedDiscoveries.
- Address Call 1 concerns in why when you keep a mixed/superficial candidate.
- Be concrete. No marketing language. No em dashes.
- Raw JSON object only.
"#;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateAssessment {
    pub id: String,
    #[serde(default)]
    pub fit: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub concerns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteGap {
    pub facet: String,
    #[serde(default)]
    pub missing_from_shortlist: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryQuery {
    pub query: String,
    #[serde(default)]
    pub target_facet: String,
    #[serde(default)]
    pub why: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CriticReport {
    #[serde(default)]
    pub candidate_assessments: Vec<CandidateAssessment>,
    #[serde(default)]
    pub taste_gaps: Vec<TasteGap>,
    #[serde(default)]
    pub discovery_queries: Vec<DiscoveryQuery>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasonerPick {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub rhymes_with: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReasonerReport {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub affinities: Vec<crate::taste::TasteAffinity>,
    #[serde(default)]
    pub aversions: Vec<crate::taste::TasteAffinity>,
    #[serde(default)]
    pub dimensions: Vec<crate::taste::TasteDimension>,
    #[serde(default)]
    pub picks: Vec<ReasonerPick>,
}

pub fn parse_critic(raw: &Value) -> Result<CriticReport, String> {
    if raw.get("picks").is_some() {
        return Err("Call 1 must not emit picks".into());
    }
    serde_json::from_value(raw.clone()).map_err(|e| format!("Call 1 JSON: {e}"))
}

pub fn parse_reasoner(raw: &Value) -> Result<ReasonerReport, String> {
    serde_json::from_value(raw.clone()).map_err(|e| format!("Call 2 JSON: {e}"))
}

fn film_sample(film: &FilmRecord) -> Option<Value> {
    let rating = film.rating?;
    Some(json!({
        "title": film.title,
        "year": film.year,
        "tmdbId": film.tmdb_id,
        "rating": rating,
        "absolute": film.signal.as_ref().map(|s| s.preference.absolute),
        "heart": film.liked,
        "viewings": film.viewings,
    }))
}

fn representative_history(films: &[FilmRecord]) -> Value {
    let mut rated: Vec<&FilmRecord> = films.iter().filter(|f| f.rating.is_some()).collect();
    rated.sort_by(|a, b| {
        b.rating
            .unwrap()
            .partial_cmp(&a.rating.unwrap())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let favorites: Vec<Value> = rated
        .iter()
        .filter(|f| f.rating.unwrap() >= 4.5)
        .take(10)
        .filter_map(|f| film_sample(f))
        .collect();
    let liked: Vec<Value> = rated
        .iter()
        .filter(|f| {
            let r = f.rating.unwrap();
            r >= 3.5 && r < 4.5
        })
        .take(10)
        .filter_map(|f| film_sample(f))
        .collect();
    let mixed: Vec<Value> = rated
        .iter()
        .filter(|f| {
            let r = f.rating.unwrap();
            r >= 2.6 && r < 3.5
        })
        .take(8)
        .filter_map(|f| film_sample(f))
        .collect();
    let disliked: Vec<Value> = rated
        .iter()
        .rev()
        .filter(|f| f.rating.unwrap() <= 2.5)
        .take(10)
        .filter_map(|f| film_sample(f))
        .collect();
    json!({
        "favorites": favorites,
        "liked": liked,
        "mixed": mixed,
        "disliked": disliked,
    })
}

fn affinity_json(profile: &FeatureProfile, positive: bool) -> Vec<Value> {
    let mut items: Vec<_> = profile
        .affinities
        .iter()
        .filter(|a| {
            let allowed_family = a.citeable();
            if !allowed_family {
                return false;
            }
            if positive {
                a.recommendation_mean > 0.08
            } else {
                a.preference_mean < -0.08 || a.negative_weight > a.positive_weight
            }
        })
        .take(12)
        .map(|a| {
            json!({
                "feature": a.key.name,
                "family": a.key.family,
                "affinity": (a.recommendation_mean as f64 * 100.0).round() / 100.0,
                "preferenceMean": (a.preference_mean as f64 * 100.0).round() / 100.0,
                "confidence": (a.confidence as f64 * 100.0).round() / 100.0,
                "portability": (a.portability as f64 * 100.0).round() / 100.0,
                "positiveEvidence": a.positive_evidence,
                "negativeEvidence": a.negative_evidence,
            })
        })
        .collect();
    if !positive {
        items.truncate(8);
    }
    items
}

fn contextual_json(profile: &FeatureProfile) -> Vec<Value> {
    profile
        .affinities
        .iter()
        .filter(|a| a.key.family.is_contextual())
        .take(8)
        .map(|a| {
            json!({
                "feature": a.key.name,
                "family": a.key.family,
                "preferenceMean": (a.preference_mean as f64 * 100.0).round() / 100.0,
                "recommendationMean": (a.recommendation_mean as f64 * 100.0).round() / 100.0,
                "portability": (a.portability as f64 * 100.0).round() / 100.0,
                "confidence": (a.confidence as f64 * 100.0).round() / 100.0,
                "note": "Catalog exposure, not a recommendation target unless portability is high.",
                "positiveEvidence": a.positive_evidence,
            })
        })
        .collect()
}

fn candidate_payload(c: &ScoredCandidate, rank: usize) -> Value {
    json!({
        "rank": rank,
        "id": c.candidate.tmdb_id.map(|id| format!("tmdb:{id}")).unwrap_or_else(|| c.candidate.title.clone()),
        "title": c.candidate.title,
        "year": c.candidate.year,
        "tmdbId": c.candidate.tmdb_id,
        "deterministicScore": (c.score.total as f64 * 100.0).round() / 100.0,
        "scoreBreakdown": {
            "content": (c.score.content as f64 * 100.0).round() / 100.0,
            "tmdbRelated": (c.score.tmdb_related as f64 * 100.0).round() / 100.0,
            "friend": (c.score.friend_affinity as f64 * 100.0).round() / 100.0,
            "recent": (c.score.recent_taste as f64 * 100.0).round() / 100.0,
            "watchlist": (c.score.watchlist as f64 * 100.0).round() / 100.0,
            "novelty": (c.score.novelty as f64 * 100.0).round() / 100.0,
            "negative": (c.score.negative_evidence as f64 * 100.0).round() / 100.0,
        },
        "reasons": c.reasons,
        "evidence": c.evidence,
        "positiveFeatures": c.positive_features,
        "negativeFeatures": c.negative_features,
        "directors": c.candidate.directors,
        "genres": c.candidate.genres,
        "modes": c.candidate.modes,
        "contextualOnly": c.contextual_only,
    })
}

pub fn call1_payload(
    films: &[FilmRecord],
    profile: &FeatureProfile,
    shortlist: &[ScoredCandidate],
) -> Value {
    json!({
        "tasteProfile": {
            "primaryAffinities": affinity_json(profile, true),
            "strongAversions": affinity_json(profile, false),
            "contextualSignals": contextual_json(profile),
            "polarizingFeatures": profile.polarizing,
            "recentChanges": profile.shifts,
            "tasteDimensions": profile.dimensions,
            "tasteModes": profile.modes,
            "modeShifts": profile.mode_shifts,
        },
        "representativeHistory": representative_history(films),
        "candidates": shortlist.iter().enumerate().map(|(i, c)| candidate_payload(c, i + 1)).collect::<Vec<_>>(),
    })
}

pub fn call2_payload(
    films: &[FilmRecord],
    profile: &FeatureProfile,
    shortlist: &[ScoredCandidate],
    critic: &CriticReport,
    discoveries: &[ScoredCandidate],
) -> Value {
    json!({
        "tasteProfile": {
            "primaryAffinities": affinity_json(profile, true),
            "strongAversions": affinity_json(profile, false),
            "contextualSignals": contextual_json(profile),
            "polarizingFeatures": profile.polarizing,
            "recentChanges": profile.shifts,
            "tasteDimensions": profile.dimensions,
            "tasteModes": profile.modes,
            "modeShifts": profile.mode_shifts,
        },
        "representativeHistory": representative_history(films),
        "originalShortlist": shortlist.iter().enumerate().map(|(i, c)| candidate_payload(c, i + 1)).collect::<Vec<_>>(),
        "call1": {
            "candidateAssessments": critic.candidate_assessments.iter().map(|a| json!({
                "id": a.id,
                "fit": a.fit,
                "reason": a.reason,
                "concerns": a.concerns,
            })).collect::<Vec<_>>(),
            "tasteGaps": critic.taste_gaps.iter().map(|g| json!({
                "facet": g.facet,
                "missingFromShortlist": g.missing_from_shortlist,
            })).collect::<Vec<_>>(),
        },
        "validatedDiscoveries": discoveries.iter().enumerate().map(|(i, c)| candidate_payload(c, i + 1)).collect::<Vec<_>>(),
        "constraints": {
            "total": 12,
            "minShortlist": 8,
            "maxDiscoveries": 3,
            "discoveryOptional": true,
        }
    })
}

pub fn payload_has_breakdown(payload: &Value) -> bool {
    payload["candidates"]
        .as_array()
        .or_else(|| payload["originalShortlist"].as_array())
        .and_then(|arr| arr.first())
        .map(|c| c.get("scoreBreakdown").is_some() && c.get("evidence").is_some() && c.get("reasons").is_some())
        .unwrap_or(false)
}

pub fn empty_critic() -> CriticReport {
    CriticReport {
        candidate_assessments: Vec::new(),
        taste_gaps: Vec::new(),
        discovery_queries: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::features::FeatureProfile;
    use crate::taste::retrieve::{RetrievalKind, RetrievalSource};
    use crate::taste::score::{CandidateScore, CandidateView};

    fn scored() -> ScoredCandidate {
        ScoredCandidate {
            candidate: CandidateView {
                tmdb_id: Some(1),
                title: "Heat".into(),
                year: Some(1995),
                poster: None,
                watchlist: false,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Related,
                    label: "x".into(),
                    seed_tmdb_id: Some(2),
                }],
                directors: vec!["Michael Mann".into()],
                genres: vec!["Crime".into()],
                modes: vec![],
            },
            score: CandidateScore {
                content: 0.7,
                tmdb_related: 0.5,
                friend_affinity: 0.1,
                recent_taste: 0.2,
                watchlist: 0.0,
                novelty: -0.1,
                negative_evidence: -0.05,
                total: 0.74,
            },
            reasons: vec!["director affinity".into()],
            evidence: vec!["Heat".into(), "Collateral".into()],
            positive_features: vec!["Michael Mann".into()],
            negative_features: vec!["Blackhat".into()],
            contextual_only: false,
        }
    }

    #[test]
    fn call1_rejects_picks() {
        let v = json!({"picks":[{"title":"Heat"}]});
        assert!(parse_critic(&v).is_err());
    }

    #[test]
    fn payloads_include_breakdown_and_evidence() {
        let profile = FeatureProfile::default();
        let p = call1_payload(&[], &profile, &[scored()]);
        assert!(payload_has_breakdown(&p));
        assert!(p["candidates"][0]["scoreBreakdown"]["content"].is_number());
        assert!(p["candidates"][0]["evidence"].as_array().unwrap().len() >= 1);
        assert!(p["tasteProfile"]["contextualSignals"].is_array());
        assert!(p["tasteProfile"]["tasteModes"].is_array());
        assert!(p["tasteProfile"]["primaryAffinities"].is_array());
        let critic = empty_critic();
        let p2 = call2_payload(&[], &profile, &[scored()], &critic, &[]);
        assert!(payload_has_breakdown(&p2));
    }

    #[test]
    fn nonportable_decade_stays_contextual() {
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
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
        let payload = call1_payload(&[], &profile, &[scored()]);
        let primary = payload["tasteProfile"]["primaryAffinities"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(primary.iter().all(|v| v["family"] != "decade"));
        let ctx = payload["tasteProfile"]["contextualSignals"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(ctx.iter().any(|v| v["feature"] == "2000s"));
    }

    #[test]
    fn singleton_person_is_not_a_primary_affinity() {
        use crate::taste::features::{build_profile, observations_from_film, Credit};
        use crate::taste::preference::{interaction_signal, rating_profile};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let s = interaction_signal(5.0, &p, Some(0.1), 1, false);
        let obs = observations_from_film(
            "The SpongeBob SquarePants Movie",
            5.0,
            Some(0),
            &s,
            Some(0.1),
            &["Animation".into()],
            &[Credit {
                id: Some(1),
                name: "Stephen Hillenburg".into(),
                job: "Director".into(),
            }],
            &[],
            Some(2004),
            None,
        );
        let profile = build_profile(&obs);
        let payload = call1_payload(&[], &profile, &[scored()]);
        let names: Vec<String> = payload["tasteProfile"]["primaryAffinities"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["feature"].as_str().map(|s| s.to_string()))
            .collect();
        assert!(
            !names.iter().any(|n| n.contains("Hillenburg")),
            "n=1 0.91 people must not enter LLM hunt context: {names:?}"
        );
    }
}
