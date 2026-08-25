use crate::catalog::tmdb::{self, MovieLookup};
use crate::storage::db::Database;
use crate::taste::features::{FeatureFamily, FeatureProfile};
use crate::taste::retrieve::{
    identity_key, Candidate, RetrievalKind, RetrievalSource,
};
use crate::taste::score::{score_candidate, ScoredCandidate};
use serde_json::Value;
use std::collections::HashSet;

pub const DISCOVERY_FLOOR: f32 = 0.12;
pub const MAX_DISCOVERIES: usize = 3;

pub fn parse_search_titles(raw: &Value) -> Vec<(String, Option<i32>)> {
    let mut out = Vec::new();
    if let Some(arr) = raw["titles"].as_array().or_else(|| raw["films"].as_array()) {
        for v in arr {
            let title = v["title"]
                .as_str()
                .or_else(|| v.as_str())
                .unwrap_or("")
                .trim();
            if title.is_empty() {
                continue;
            }
            let year = v["year"].as_i64().map(|y| y as i32);
            out.push((title.to_string(), year));
        }
    }
    out.truncate(8);
    out
}

pub fn materialize(
    db: &Database,
    titles: &[(String, Option<i32>)],
    query: &str,
    seen: &HashSet<String>,
    profile: &FeatureProfile,
) -> Vec<ScoredCandidate> {
    let mut out = Vec::new();
    for (title, year) in titles {
        let Ok(Some(hit)) = tmdb::lookup_movie(title, *year) else {
            continue;
        };
        if let Some(scored) = score_lookup(db, hit, query, seen, profile) {
            out.push(scored);
        }
        if out.len() >= MAX_DISCOVERIES {
            break;
        }
    }
    out
}

fn score_lookup(
    db: &Database,
    hit: MovieLookup,
    query: &str,
    seen: &HashSet<String>,
    profile: &FeatureProfile,
) -> Option<ScoredCandidate> {
    let key = identity_key(Some(hit.tmdb_id), &hit.title, hit.year);
    if seen.contains(&key) {
        return None;
    }
    let _ = tmdb::refresh_movie_catalog(db, hit.tmdb_id, false);
    let mut candidate = Candidate {
        tmdb_id: Some(hit.tmdb_id),
        title: hit.title,
        year: hit.year,
        poster: hit.poster,
        genres: Vec::new(),
        credits: Vec::new(),
        keywords: Vec::new(),
        runtime: None,
        vote_count: None,
        watchlist: false,
        sources: vec![RetrievalSource {
            kind: RetrievalKind::Discovery,
            label: query.to_string(),
            seed_tmdb_id: None,
        }],
        friend_affinity: 0.0,
        tmdb_related: 0.0,
    };
    let _ = crate::taste::retrieve::enrich_missing(db, std::slice::from_mut(&mut candidate), 1);
    let scored = score_candidate(profile, &candidate);
    let sparse_facet = matches_sparse_facet(profile, &scored);
    if scored.score.total >= DISCOVERY_FLOOR || sparse_facet {
        Some(scored)
    } else {
        None
    }
}

fn matches_sparse_facet(profile: &FeatureProfile, scored: &ScoredCandidate) -> bool {
    profile.affinities.iter().any(|a| {
        a.appearances <= 4
            && a.confidence > 0.35
            && a.recommendation_mean > 0.2
            && matches!(
                a.key.family,
                FeatureFamily::Keyword | FeatureFamily::Genre | FeatureFamily::Director
            )
            && (scored.positive_features.iter().any(|n| n == &a.key.name)
                || scored.candidate.genres.iter().any(|g| g == &a.key.name)
                || scored.candidate.directors.iter().any(|d| d == &a.key.name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_titles() {
        let v = json!({"titles":[{"title":"Zodiac","year":2007},{"title":"The Insider"}]});
        let t = parse_search_titles(&v);
        assert_eq!(t[0].0, "Zodiac");
        assert_eq!(t[0].1, Some(2007));
    }

    #[test]
    fn floor_constant() {
        assert!(DISCOVERY_FLOOR > 0.0);
    }
}
