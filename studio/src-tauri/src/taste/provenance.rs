use crate::taste::score::CandidateScore;
use crate::taste::retrieve::{RetrievalKind, RetrievalSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecommendationOrigin {
    Shortlist,
    Discovery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecommendationMode {
    Core,
    DeepCut,
    Adjacent,
    Discovery,
}

impl RecommendationMode {
    pub fn parse(raw: &str) -> Self {
        match raw.to_ascii_lowercase().as_str() {
            "deepcut" | "deep cut" | "deep-cut" => Self::DeepCut,
            "adjacent" => Self::Adjacent,
            "discovery" => Self::Discovery,
            _ => Self::Core,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationProvenance {
    pub tmdb_id: Option<i64>,
    pub origin: RecommendationOrigin,
    pub retrieval_sources: Vec<RetrievalSource>,
    pub deterministic_score: CandidateScore,
    pub llm_mode: RecommendationMode,
    pub seed_films: Vec<String>,
    pub positive_features: Vec<String>,
    pub negative_features_considered: Vec<String>,
    pub call1_fit: Option<String>,
    pub call1_concerns: Vec<String>,
}

pub fn origin_from_sources(sources: &[RetrievalSource]) -> RecommendationOrigin {
    if sources.iter().any(|s| s.kind == RetrievalKind::Discovery) {
        RecommendationOrigin::Discovery
    } else {
        RecommendationOrigin::Shortlist
    }
}
