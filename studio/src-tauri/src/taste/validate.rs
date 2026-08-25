use crate::taste::reason::ReasonerPick;
use crate::taste::retrieve::identity_key;
use crate::taste::score::ScoredCandidate;
use std::collections::{HashMap, HashSet};

pub const MIN_SHORTLIST_PICKS: usize = 8;
pub const MAX_DISCOVERY_PICKS: usize = 3;
pub const TARGET_PICKS: usize = 12;

#[derive(Debug, Clone)]
pub struct DiversityWarning {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub picks: Vec<ScoredCandidate>,
    pub dropped: Vec<String>,
    pub warnings: Vec<DiversityWarning>,
    pub narrow_profile: bool,
}

pub fn hard_validate(
    picks: &[ReasonerPick],
    shortlist: &[ScoredCandidate],
    discoveries: &[ScoredCandidate],
    seen: &HashSet<String>,
    profile_mode_count: usize,
) -> ValidationResult {
    let mut allowed: HashMap<String, &ScoredCandidate> = HashMap::new();
    for c in shortlist.iter().chain(discoveries.iter()) {
        allowed.insert(cand_id(c), c);
        if let Some(id) = c.candidate.tmdb_id {
            allowed.insert(format!("tmdb:{id}"), c);
        }
        allowed.insert(c.candidate.title.to_lowercase(), c);
    }

    let mut used = HashSet::new();
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    let mut _shortlist_n = 0;
    let mut discovery_n = 0;

    for pick in picks {
        let Some(scored) = resolve_pick(pick, &allowed) else {
            dropped.push(format!("hallucinated: {}", pick.title));
            continue;
        };
        let key = identity_key(
            scored.candidate.tmdb_id,
            &scored.candidate.title,
            scored.candidate.year,
        );
        if scored.contextual_only {
            dropped.push(format!("contextual-only: {}", scored.candidate.title));
            continue;
        }
        if seen.contains(&key) {
            dropped.push(format!("seen: {}", scored.candidate.title));
            continue;
        }
        if scored.candidate.tmdb_id.is_none() {
            dropped.push(format!("invalid identity: {}", scored.candidate.title));
            continue;
        }
        if !used.insert(key) {
            dropped.push(format!("duplicate: {}", scored.candidate.title));
            continue;
        }
        let is_discovery = discoveries.iter().any(|d| same_film(d, scored));
        if is_discovery {
            if discovery_n >= MAX_DISCOVERY_PICKS {
                dropped.push(format!("extra discovery: {}", scored.candidate.title));
                continue;
            }
            discovery_n += 1;
        } else {
            _shortlist_n += 1;
        }
        let clone = scored.clone();
        kept.push(clone);
        if kept.len() >= TARGET_PICKS {
            break;
        }
    }

    if kept.iter().filter(|k| !discoveries.iter().any(|d| same_film(d, k))).count() < MIN_SHORTLIST_PICKS
    {
        for c in shortlist {
            if kept.len() >= TARGET_PICKS {
                break;
            }
            let key = identity_key(
                c.candidate.tmdb_id,
                &c.candidate.title,
                c.candidate.year,
            );
            if seen.contains(&key) || !used.insert(key) {
                continue;
            }
            if c.candidate.tmdb_id.is_none() || c.contextual_only {
                continue;
            }
            kept.push(c.clone());
        }
    }

    while kept.len() < TARGET_PICKS {
        let Some(next) = shortlist.iter().find(|c| {
            let key = identity_key(c.candidate.tmdb_id, &c.candidate.title, c.candidate.year);
            c.candidate.tmdb_id.is_some()
                && !c.contextual_only
                && !seen.contains(&key)
                && !used.contains(&key)
        }) else {
            break;
        };
        let key = identity_key(
            next.candidate.tmdb_id,
            &next.candidate.title,
            next.candidate.year,
        );
        used.insert(key);
        kept.push(next.clone());
    }

    let warnings = diversity_warnings(&kept);
    let narrow = profile_mode_count <= 2 || is_narrow(&kept);
    ValidationResult {
        picks: kept,
        dropped,
        warnings,
        narrow_profile: narrow,
    }
}

pub fn diversity_warnings(picks: &[ScoredCandidate]) -> Vec<DiversityWarning> {
    let mut dirs: HashMap<String, u32> = HashMap::new();
    let mut genres: HashMap<String, u32> = HashMap::new();
    let mut modes: HashMap<String, u32> = HashMap::new();
    let mut people: HashMap<String, u32> = HashMap::new();
    for p in picks {
        for d in &p.candidate.directors {
            *dirs.entry(d.clone()).or_insert(0) += 1;
        }
        if let Some(g) = p.candidate.genres.first() {
            *genres.entry(g.clone()).or_insert(0) += 1;
        }
        if let Some(m) = p.candidate.modes.first() {
            *modes.entry(m.clone()).or_insert(0) += 1;
        }
        for f in &p.person_keys {
            *people.entry(f.clone()).or_insert(0) += 1;
        }
    }
    let mut out = Vec::new();
    for (d, n) in dirs {
        if n > 3 {
            out.push(DiversityWarning {
                message: format!("{n} films share director {d}"),
            });
        }
    }
    for (g, n) in genres {
        if n > 4 {
            out.push(DiversityWarning {
                message: format!("{n} films share primary genre {g}"),
            });
        }
    }
    for (m, n) in modes {
        if n > 5 {
            out.push(DiversityWarning {
                message: format!("{n} films share taste mode {m}"),
            });
        }
    }
    for (person, n) in people {
        if n > 3 {
            out.push(DiversityWarning {
                message: format!("{n} films share person {person}"),
            });
        }
    }
    out
}

fn is_narrow(picks: &[ScoredCandidate]) -> bool {
    if picks.len() < 6 {
        return false;
    }
    let mut dirs: HashMap<String, u32> = HashMap::new();
    for p in picks {
        for d in &p.candidate.directors {
            *dirs.entry(d.clone()).or_insert(0) += 1;
        }
    }
    dirs.values().any(|n| *n as usize * 2 >= picks.len())
}

fn cand_id(c: &ScoredCandidate) -> String {
    c.candidate
        .tmdb_id
        .map(|id| format!("tmdb:{id}"))
        .unwrap_or_else(|| c.candidate.title.to_lowercase())
}

fn resolve_pick<'a>(
    pick: &ReasonerPick,
    allowed: &HashMap<String, &'a ScoredCandidate>,
) -> Option<&'a ScoredCandidate> {
    if !pick.id.is_empty() {
        if let Some(c) = allowed.get(&pick.id) {
            return Some(*c);
        }
    }
    allowed.get(&pick.title.to_lowercase()).copied()
}

fn same_film(a: &ScoredCandidate, b: &ScoredCandidate) -> bool {
    match (a.candidate.tmdb_id, b.candidate.tmdb_id) {
        (Some(x), Some(y)) => x == y,
        _ => a.candidate.title.eq_ignore_ascii_case(&b.candidate.title),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::retrieve::{RetrievalKind, RetrievalSource};
    use crate::taste::score::{CandidateScore, CandidateView};

    fn scored(id: i64, title: &str, director: &str) -> ScoredCandidate {
        ScoredCandidate {
            candidate: CandidateView {
                tmdb_id: Some(id),
                title: title.into(),
                year: Some(1999),
                poster: None,
                watchlist: false,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Related,
                    label: "x".into(),
                    seed_tmdb_id: None,
                }],
                directors: vec![director.into()],
                genres: vec!["Crime".into()],
                modes: vec![],
            },
            score: CandidateScore {
                content: 0.5,
                tmdb_related: 0.4,
                friend_affinity: 0.0,
                recent_taste: 0.0,
                watchlist: 0.0,
                novelty: 0.0,
                negative_evidence: 0.0,
                total: 0.5,
            },
            reasons: vec!["r".into()],
            evidence: vec!["Heat".into()],
            positive_features: vec![],
            negative_features: vec![],
            contextual_only: false,
            person_keys: vec![],
        }
    }

    fn pick(id: i64, title: &str) -> ReasonerPick {
        ReasonerPick {
            id: format!("tmdb:{id}"),
            title: title.into(),
            year: Some(1999),
            why: "because".into(),
            mode: "core".into(),
            rhymes_with: vec![],
        }
    }

    #[test]
    fn drops_hallucinations_and_seen() {
        let short = vec![scored(1, "Heat", "Mann"), scored(2, "Thief", "Mann")];
        let picks = vec![
            pick(1, "Heat"),
            pick(99, "Invented"),
        ];
        let mut seen = HashSet::new();
        seen.insert("tmdb:1".into());
        let result = hard_validate(&picks, &short, &[], &seen, 4);
        assert!(result.dropped.iter().any(|d| d.contains("seen")));
        assert!(result.dropped.iter().any(|d| d.contains("hallucinated")));
        assert!(result.picks.iter().any(|p| p.candidate.tmdb_id == Some(2)));
    }

    #[test]
    fn does_not_silent_swap_on_diversity() {
        let short: Vec<_> = (1..20)
            .map(|i| scored(i, &format!("Film {i}"), "Mann"))
            .collect();
        let picks: Vec<_> = (1..13).map(|i| pick(i, &format!("Film {i}"))).collect();
        let result = hard_validate(&picks, &short, &[], &HashSet::new(), 4);
        assert_eq!(result.picks.len(), 12);
        assert!(!result.warnings.is_empty());
        assert_eq!(result.picks[0].candidate.tmdb_id, Some(1));
    }

    #[test]
    fn drops_contextual_only_core_picks() {
        let mut dirty = scored(3, "Dirty", "Other");
        dirty.contextual_only = true;
        dirty.reasons = vec!["2000s affinity (0.25)".into()];
        let short = vec![scored(1, "Heat", "Mann"), scored(2, "Thief", "Mann"), dirty];
        let picks = vec![pick(3, "Dirty"), pick(1, "Heat")];
        let result = hard_validate(&picks, &short, &[], &HashSet::new(), 4);
        assert!(result.dropped.iter().any(|d| d.contains("contextual-only")));
        assert!(result.picks.iter().all(|p| p.candidate.tmdb_id != Some(3)));
        assert!(result.picks.iter().any(|p| p.candidate.tmdb_id == Some(1)));
    }

    #[test]
    fn warns_when_one_person_dominates_the_final_12() {
        let mut picks = Vec::new();
        for i in 1..13 {
            let mut c = scored(i, &format!("Film {i}"), "Other");
            c.positive_features = vec!["John Powell".into()];
            c.person_keys = vec!["John Powell".into()];
            picks.push(c);
        }
        let warnings = diversity_warnings(&picks);
        assert!(
            warnings.iter().any(|w| w.message.contains("person John Powell")),
            "{:?}",
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}
