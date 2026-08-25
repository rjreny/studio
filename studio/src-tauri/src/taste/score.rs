use crate::taste::features::{
    decade_label, family_for_job, keyword_is_taste_signal, keyword_role, runtime_bucket,
    EvidenceFilm, FeatureAffinity, FeatureFamily, FeatureKey, FeatureProfile, KeywordRole,
    PORTABLE_CONTEXTUAL,
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
    #[serde(default)]
    pub person_keys: Vec<String>,
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
        aff.citeable()
            && aff.key.family.is_primary()
            && keys.iter().any(|k| k.storage_key() == aff.key.storage_key())
    });
    let mut family_used: std::collections::HashMap<FeatureFamily, usize> =
        std::collections::HashMap::new();
    let mut cited: Vec<&FeatureAffinity> = Vec::new();

    for aff in &profile.affinities {
        if aff.key.family.is_contextual() || !aff.citeable() {
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
        }
        if aff.recommendation_mean > 0.1 {
            cited.push(aff);
        }
    }

    let specific = cited.iter().any(|a| a.key.is_person_or_keyword());
    let cited: Vec<_> = cited
        .into_iter()
        .filter(|a| !specific || a.key.is_person_or_keyword())
        .collect();
    let mut person_keys = Vec::new();
    for aff in &cited {
        if matches!(
            aff.key.family,
            FeatureFamily::Director
                | FeatureFamily::Writer
                | FeatureFamily::Cinematographer
                | FeatureFamily::Composer
                | FeatureFamily::Actor
        ) {
            person_keys.push(aff.key.name.clone());
        }
        positive_features.push(aff.key.name.clone());
        if reasons.len() < 4 {
            reasons.push(format!(
                "{} affinity ({:.2})",
                aff.key.name, aff.recommendation_mean
            ));
        }
        for title in evidence_titles_for(aff, candidate) {
            if evidence.len() < 6 && !evidence.iter().any(|e| e == &title) {
                evidence.push(title);
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
        contextual_only: !matched_primary
            || (candidate.watchlist && !watchlist_has_strong_bridge(&cited, candidate))
            || (is_related(candidate) && !related_has_portable_bridge(&cited, candidate)),
        person_keys,
    }
}

/// Watchlist is real intent (trailers, friends, plan-to-watch) but only a subtle
/// boost (`W_WATCHLIST`). Eligibility is about *feature usefulness*, not an
/// affinity floor:
/// - citeable craft (director/writer/DP/composer) or signal keyword → eligible
/// - actor needs `recommendation_mean >= PORTABLE_CONTEXTUAL` so extras/cameos
///   cannot occupy the same real estate as evidence-backed recommendations
/// - genre alone is never a watchlist bridge
/// Frozen formulas and `W_WATCHLIST` are unchanged.
fn is_related(candidate: &Candidate) -> bool {
    candidate
        .sources
        .iter()
        .any(|s| s.kind == RetrievalKind::Related)
}

/// TMDB-related is useful when it finds a person or signal keyword already in
/// the profile. Broad genre overlap is a description of the similar-to seed,
/// not a recommendation signal. Thin-but-citeable craft (Fraser at 0.37) still
/// counts; this does not apply the actor affinity floor. A craft credit still
/// has to transfer from liked evidence to this particular film.
fn related_has_portable_bridge(cited: &[&FeatureAffinity], candidate: &Candidate) -> bool {
    cited.iter().any(|a| match a.key.family {
        FeatureFamily::Keyword => true,
        FeatureFamily::Director
        | FeatureFamily::Writer
        | FeatureFamily::Cinematographer
        | FeatureFamily::Composer => person_relevance_transfers(a, candidate),
        _ => a.key.is_person_or_keyword(),
    })
}

fn watchlist_has_strong_bridge(cited: &[&FeatureAffinity], candidate: &Candidate) -> bool {
    cited.iter().any(|a| match a.key.family {
        FeatureFamily::Keyword => true,
        FeatureFamily::Director
        | FeatureFamily::Writer
        | FeatureFamily::Cinematographer
        | FeatureFamily::Composer => person_relevance_transfers(a, candidate),
        FeatureFamily::Actor => a.recommendation_mean >= PORTABLE_CONTEXTUAL,
        _ => false,
    })
}

pub(crate) fn reasons_have_strong_bridge(reasons: &[String]) -> bool {
    reasons.iter().any(|r| {
        let lower = r.to_lowercase();
        lower.contains("affinity") && !is_broad_genre_reason(&lower)
    })
}

pub(crate) fn reasons_are_genre_only(reasons: &[String]) -> bool {
    let affinities: Vec<_> = reasons
        .iter()
        .filter(|r| r.to_lowercase().contains("affinity"))
        .collect();
    !affinities.is_empty()
        && affinities
            .iter()
            .all(|r| is_broad_genre_reason(&r.to_lowercase()))
}

fn is_broad_genre_reason(reason: &str) -> bool {
    let Some(label) = reason.split(" affinity").next() else {
        return false;
    };
    matches!(
        label,
        "drama"
            | "comedy"
            | "romance"
            | "thriller"
            | "crime"
            | "action"
            | "family"
            | "horror"
            | "adventure"
            | "fantasy"
            | "science fiction"
            | "animation"
            | "mystery"
            | "history"
            | "war"
            | "music"
            | "western"
            | "documentary"
            | "tv movie"
    )
}

/// Filmography is an investigate-the-person query. A credit is eligible when
/// the person's relevance transfers from liked films to this candidate:
/// watchlist creator loyalty (unenriched title), cluster overlap with liked
/// examples, two shared genres with a liked example, or — when the cluster is
/// empty — any genre overlap with a liked example. Watchlist does not bypass
/// that test. An independent citeable signal keyword can still rescue a miss.
/// Frozen affinities are unchanged.
pub fn filmography_supported(profile: &FeatureProfile, candidate: &Candidate) -> bool {
    let Some(src) = candidate
        .sources
        .iter()
        .find(|s| s.kind == RetrievalKind::Filmography)
    else {
        return true;
    };
    let Some(aff) = profile.affinities.iter().find(|a| {
        a.citeable()
            && a.key.is_person_or_keyword()
            && a.key.name.eq_ignore_ascii_case(&src.label)
    }) else {
        return true;
    };
    person_relevance_transfers(aff, candidate)
        || independent_portable_signal(profile, candidate)
}

/// Does this person's liked work make *this* film interesting — not merely
/// "the user likes this person, so dump the catalog"?
fn person_relevance_transfers(aff: &FeatureAffinity, candidate: &Candidate) -> bool {
    // Unenriched watchlist titles (Piranesi, unreleased Laika) keep creator
    // loyalty. Filmography retrieval of the same empty-metadata credit is
    // generic catalog expansion and must not use this path.
    if candidate.watchlist
        && candidate_unenriched(candidate)
        && aff.positive_evidence.len() >= 2
    {
        return true;
    }
    let modes = predicted_modes(
        &candidate.genres,
        &candidate.credits,
        &candidate.keywords,
    );
    if !aff.evidence_cluster.is_empty()
        && aff
            .evidence_cluster
            .overlaps(&candidate.genres, &candidate.keywords, &modes)
    {
        return true;
    }
    // A minority Drama tag on Real Steel must not carry Schindler's List, but
    // two shared genres with a liked example (Batman ↔ The Gambler) is transfer.
    if aff
        .positive_evidence
        .iter()
        .any(|film| genre_overlap_count(film, candidate) >= 2)
    {
        return true;
    }
    if aff.evidence_cluster.is_empty() {
        return aff
            .positive_evidence
            .iter()
            .any(|film| evidence_resembles_candidate(film, candidate));
    }
    false
}

fn candidate_unenriched(candidate: &Candidate) -> bool {
    candidate.genres.is_empty() && candidate.keywords.is_empty()
}

fn evidence_resembles_candidate(film: &EvidenceFilm, candidate: &Candidate) -> bool {
    genre_overlap_count(film, candidate) > 0 || keyword_overlap(film, candidate)
}

fn genre_overlap_count(film: &EvidenceFilm, candidate: &Candidate) -> usize {
    film.genres
        .iter()
        .filter(|g| {
            candidate
                .genres
                .iter()
                .any(|c| c.eq_ignore_ascii_case(g))
        })
        .count()
}

fn keyword_overlap(film: &EvidenceFilm, candidate: &Candidate) -> bool {
    film.keywords.iter().any(|k| {
        keyword_role(k) == KeywordRole::Signal
            && candidate
                .keywords
                .iter()
                .any(|ck| ck.name.eq_ignore_ascii_case(k))
    })
}

fn independent_portable_signal(profile: &FeatureProfile, candidate: &Candidate) -> bool {
    candidate.keywords.iter().any(|k| {
        keyword_role(&k.name) == KeywordRole::Signal
            && profile.affinities.iter().any(|a| {
                a.citeable()
                    && a.key.family == FeatureFamily::Keyword
                    && a.key.name.eq_ignore_ascii_case(&k.name)
            })
    })
}

fn evidence_titles_for(aff: &FeatureAffinity, candidate: &Candidate) -> Vec<String> {
    aff.positive_evidence
        .iter()
        .filter(|film| evidence_fits_candidate(aff, film, candidate))
        .map(|film| film.title.clone())
        .take(2)
        .collect()
}

/// Genre matches are too broad to inherit the strongest observations of that
/// genre. A Drama candidate only cites a Drama evidence film when they share a
/// person or a quality keyword — not merely the genre.
fn evidence_fits_candidate(
    aff: &FeatureAffinity,
    film: &crate::taste::features::EvidenceFilm,
    candidate: &Candidate,
) -> bool {
    match aff.key.family {
        FeatureFamily::Genre => {
            let person_hit = film.people.iter().any(|p| {
                candidate
                    .credits
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(p))
            });
            let keyword_hit = film.keywords.iter().any(|k| {
                keyword_role(k) == KeywordRole::Signal
                    && candidate
                        .keywords
                        .iter()
                        .any(|ck| ck.name.eq_ignore_ascii_case(k))
            });
            person_hit || keyword_hit
        }
        FeatureFamily::Keyword => keyword_role(&aff.key.name) == KeywordRole::Signal,
        _ => true,
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
        if !keyword_is_taste_signal(&k.name) {
            continue;
        }
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
        .filter(|c| filmography_supported(profile, c))
        .map(|c| score_candidate(profile, c))
        .filter(|c| !c.contextual_only)
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .total
            .partial_cmp(&a.score.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cap_filmography_per_person(&mut scored, 4);
    scored.truncate(100);
    scored
}

fn cap_filmography_per_person(scored: &mut Vec<ScoredCandidate>, max_n: usize) {
    use std::collections::HashMap;
    let mut kept: HashMap<String, usize> = HashMap::new();
    scored.retain(|c| {
        let Some(src) = c
            .candidate
            .sources
            .iter()
            .find(|s| s.kind == RetrievalKind::Filmography)
        else {
            return true;
        };
        let n = kept.entry(src.label.clone()).or_insert(0);
        if *n >= max_n {
            return false;
        }
        *n += 1;
        true
    });
}

#[derive(Debug, Clone)]
pub struct PersonRetrievalTrace {
    pub name: String,
    pub appearances: u32,
    pub recommendation_mean: f32,
    pub confidence: f32,
    pub scoring_affinity: f32,
    pub injected: usize,
    pub survived_score: usize,
    pub survived_mmr: usize,
}

pub fn person_pipeline_trace(
    profile: &FeatureProfile,
    injected: &[Candidate],
    scored: &[ScoredCandidate],
    shortlist: &[ScoredCandidate],
) -> Vec<PersonRetrievalTrace> {
    profile
        .affinities
        .iter()
        .filter(|a| a.citeable() && a.key.is_person_or_keyword())
        .map(|a| {
            let named = |credits: &[crate::taste::features::Credit]| {
                credits.iter().any(|c| c.name.eq_ignore_ascii_case(&a.key.name))
            };
            let injected_n = injected
                .iter()
                .filter(|c| {
                    named(&c.credits)
                        || c.sources.iter().any(|s| {
                            s.kind == RetrievalKind::Filmography
                                && s.label.eq_ignore_ascii_case(&a.key.name)
                        })
                })
                .count();
            let in_list = |rows: &[ScoredCandidate]| {
                rows.iter()
                    .filter(|c| {
                        c.positive_features
                            .iter()
                            .any(|f| f.eq_ignore_ascii_case(&a.key.name))
                    })
                    .count()
            };
            PersonRetrievalTrace {
                name: a.key.name.clone(),
                appearances: a.appearances,
                recommendation_mean: a.recommendation_mean,
                confidence: a.confidence,
                scoring_affinity: a.scoring_affinity(),
                injected: injected_n,
                survived_score: in_list(scored),
                survived_mmr: in_list(shortlist),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::features::{build_profile, observations_from_film, Credit};
    use crate::taste::preference::{interaction_signal, rating_profile};
    use crate::taste::retrieve::{RetrievalKind, RetrievalSource};

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

    #[test]
    fn person_evidence_does_not_bleed_genre_evidence() {
        use crate::taste::features::{build_profile, observations_from_film, Credit};
        use crate::taste::preference::{interaction_signal, rating_profile};
        use crate::taste::retrieve::{Candidate, RetrievalKind, RetrievalSource};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let director = Credit {
            id: Some(10),
            name: "Stephen Hillenburg".into(),
            job: "Director".into(),
        };
        let mut obs = observations_from_film(
            "The SpongeBob SquarePants Movie",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(1.0), 1, false),
            Some(1.0),
            &["Comedy".into(), "Family".into()],
            &[director.clone()],
            &[],
            Some(2004),
            Some(87),
        );
        obs.extend(observations_from_film(
            "SpongeBob extra",
            4.5,
            Some(3),
            &interaction_signal(4.5, &p, Some(1.0), 1, false),
            Some(1.0),
            &["Comedy".into()],
            &[director.clone()],
            &[],
            Some(2015),
            Some(90),
        ));
        obs.extend(observations_from_film(
            "The Twilight Saga: Breaking Dawn - Part 1",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Drama".into(), "Fantasy".into()],
            &[Credit {
                id: Some(99),
                name: "Bill Condon".into(),
                job: "Director".into(),
            }],
            &[],
            Some(2011),
            Some(117),
        ));
        let profile = build_profile(&obs);
        let cand = Candidate {
            tmdb_id: Some(50),
            title: "The SpongeBob Movie: Sponge Out of Water".into(),
            year: Some(2015),
            poster: None,
            genres: vec!["Comedy".into(), "Drama".into()],
            credits: vec![director],
            keywords: vec![],
            runtime: Some(92),
            vote_count: Some(1000),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Filmography,
                label: "Stephen Hillenburg".into(),
                seed_tmdb_id: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        };
        let scored = score_candidate(&profile, &cand);
        assert!(scored.evidence.iter().any(|e| e.contains("SpongeBob")));
        assert!(
            scored.evidence.iter().all(|e| !e.contains("Twilight")),
            "got {:?}",
            scored.evidence
        );
        assert!(scored.reasons.iter().any(|r| r.contains("Hillenburg")));
        assert!(scored.reasons.iter().all(|r| !r.contains("2000s")));
        assert!(scored.positive_features.iter().any(|f| f.contains("Hillenburg")));
        assert!(
            scored.positive_features.iter().all(|f| f != "Drama"),
            "genre must not ride along with person evidence: {:?}",
            scored.positive_features
        );
    }

    fn watchlist_drama(title: &str, tmdb_id: i64) -> Candidate {
        Candidate {
            tmdb_id: Some(tmdb_id),
            title: title.into(),
            year: Some(1962),
            poster: None,
            genres: vec!["Drama".into()],
            credits: vec![],
            keywords: vec![],
            runtime: Some(129),
            vote_count: Some(5000),
            watchlist: true,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Watchlist,
                label: "watchlist".into(),
                seed_tmdb_id: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        }
    }

    fn related_genre(title: &str, tmdb_id: i64, genres: &[&str]) -> Candidate {
        Candidate {
            tmdb_id: Some(tmdb_id),
            title: title.into(),
            year: Some(2012),
            poster: None,
            genres: genres.iter().map(|g| (*g).into()).collect(),
            credits: vec![],
            keywords: vec![],
            runtime: Some(110),
            vote_count: Some(800),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar to a liked drama".into(),
                seed_tmdb_id: Some(1),
            }],
            friend_affinity: 0.0,
            tmdb_related: 1.0,
        }
    }

    /// Real 627-film run: To Kill a Mockingbird / Sunset Boulevard / 12 Angry Men
    /// were recommended as Drama watchlist items with evidence=["Twilight", ...].
    #[test]
    fn watchlist_drama_does_not_use_twilight_as_universal_evidence() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let mut obs = observations_from_film(
            "Twilight",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Drama".into(), "Fantasy".into()],
            &[Credit {
                id: Some(99),
                name: "Bill Condon".into(),
                job: "Director".into(),
            }],
            &[],
            Some(2008),
            Some(122),
        );
        obs.extend(observations_from_film(
            "The Twilight Saga: Breaking Dawn - Part 2",
            4.5,
            Some(3),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Drama".into(), "Fantasy".into()],
            &[Credit {
                id: Some(99),
                name: "Bill Condon".into(),
                job: "Director".into(),
            }],
            &[],
            Some(2012),
            Some(115),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("other{i}"),
                4.5,
                Some(20 + i),
                &s,
                Some(0.4),
                &["Comedy".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let scored = score_candidate(&profile, &watchlist_drama("To Kill a Mockingbird", 941));
        assert!(
            scored.evidence.iter().all(|e| !e.to_lowercase().contains("twilight")),
            "Twilight must not be drama-universal evidence, got {:?}",
            scored.evidence
        );
        assert!(scored.reasons.iter().all(|r| !r.to_lowercase().contains("twilight")));
    }

    fn drama_and_fraser_profile() -> (crate::taste::features::FeatureProfile, Credit) {
        drama_fraser_and_weak_actor().0
    }

    /// Real 627-film run after genre-only drop: 27/50 were still watchlist films
    /// whose only bridge was a weak citeable actor (Ruffalo 0.27, Ferrell 0.13).
    fn drama_fraser_and_weak_actor() -> (
        (crate::taste::features::FeatureProfile, Credit),
        Credit,
    ) {
        use crate::taste::features::PORTABLE_CONTEXTUAL;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let dp = Credit {
            id: Some(77),
            name: "Greig Fraser".into(),
            job: "Director of Photography".into(),
        };
        let actor = Credit {
            id: Some(88),
            name: "Mark Ruffalo".into(),
            job: "Actor".into(),
        };
        let mut obs = observations_from_film(
            "The Batman",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2022),
            Some(176),
        );
        obs.extend(observations_from_film(
            "Rogue One: A Star Wars Story",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2016),
            Some(133),
        ));
        obs.extend(observations_from_film(
            "Spotlight",
            3.5,
            Some(3),
            &interaction_signal(3.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Drama".into()],
            &[actor.clone()],
            &[],
            Some(2015),
            Some(129),
        ));
        obs.extend(observations_from_film(
            "Zodiac",
            3.5,
            Some(4),
            &interaction_signal(3.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Crime".into(), "Drama".into()],
            &[actor.clone()],
            &[],
            Some(2007),
            Some(157),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("drama{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let ruffalo = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Mark Ruffalo")
            .expect("Ruffalo");
        assert!(ruffalo.citeable(), "weak actor must still be citeable");
        assert!(
            ruffalo.scoring_affinity() < PORTABLE_CONTEXTUAL,
            "fixture actor should be below the watchlist bridge bar, got {}",
            ruffalo.scoring_affinity()
        );
        ((profile, dp), actor)
    }

    /// Real 627-film run: 31/50 were watchlist + Drama with empty evidence.
    #[test]
    fn watchlist_drama_only_cannot_dominate_shortlist() {
        use crate::taste::shortlist::shortlist;
        let (profile, dp) = drama_and_fraser_profile();
        let mut cands = Vec::new();
        for i in 0..20i64 {
            let mut c = watchlist_drama(&format!("Shelf Drama {i}"), 400 + i);
            c.year = Some(1960 + i as i32);
            cands.push(c);
        }
        cands.push({
            let mut c = watchlist_drama("The Gambler", 77);
            c.credits = vec![dp.clone()];
            c
        });
        cands.push(Candidate {
            tmdb_id: Some(78),
            title: "Dune".into(),
            year: Some(2021),
            poster: None,
            genres: vec!["Science Fiction".into()],
            credits: vec![dp],
            keywords: vec![],
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
        });
        let scored = score_all(&profile, &cands);
        let short = shortlist(&scored);
        let weak = short
            .iter()
            .filter(|c| {
                c.candidate.watchlist
                    && c.person_keys.is_empty()
                    && c.candidate.title.starts_with("Shelf Drama")
            })
            .count();
        assert_eq!(
            weak, 0,
            "watchlist+genre-only must not occupy the shortlist, got {weak} of {}",
            short.len()
        );
        assert!(
            scored.iter().any(|c| c.candidate.title == "The Gambler" && !c.contextual_only),
            "watchlist+Fraser must remain eligible"
        );
    }

    #[test]
    fn watchlist_with_portable_person_remains_eligible() {
        let (profile, dp) = drama_and_fraser_profile();
        let mut cand = watchlist_drama("The Gambler", 77);
        cand.credits = vec![dp];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "watchlist + Fraser is portable, not genre-only"
        );
        assert!(scored.person_keys.iter().any(|p| p.contains("Fraser")));
        let pool = score_all(&profile, &[cand]);
        assert_eq!(pool.len(), 1);
    }

    /// 2-film Fraser at the user's mean (~0.375 rec mean) must stay eligible.
    /// A blanket 0.4 affinity floor would throw away a citeable cinematographer.
    #[test]
    fn watchlist_thin_fraser_remains_eligible() {
        use crate::taste::features::PORTABLE_CONTEXTUAL;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let dp = Credit {
            id: Some(77),
            name: "Greig Fraser".into(),
            job: "Director of Photography".into(),
        };
        let mut obs = observations_from_film(
            "The Batman",
            4.0,
            Some(1),
            &interaction_signal(4.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2022),
            Some(176),
        );
        obs.extend(observations_from_film(
            "Rogue One: A Star Wars Story",
            4.0,
            Some(2),
            &interaction_signal(4.0, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2016),
            Some(133),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("drama{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let fraser = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .expect("Fraser");
        assert!(fraser.citeable(), "thin Fraser must remain a useful feature");
        assert!(
            fraser.recommendation_mean < PORTABLE_CONTEXTUAL,
            "fixture must sit below the actor floor, got rec_mean={}",
            fraser.recommendation_mean
        );
        assert!(
            fraser.recommendation_mean > 0.1,
            "Fraser must still be citeable in scoring, got {}",
            fraser.recommendation_mean
        );
        let mut cand = watchlist_drama("The Gambler", 77);
        cand.credits = vec![dp];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "citeable Fraser must not be dropped by an affinity floor, reasons={:?} rec_mean={}",
            scored.reasons,
            fraser.recommendation_mean
        );
        assert_eq!(score_all(&profile, &[cand]).len(), 1);
    }

    /// 2-film neo-noir at the user's mean must stay usable on a watchlist title.
    #[test]
    fn watchlist_thin_neo_noir_remains_eligible() {
        use crate::taste::features::{Keyword, PORTABLE_CONTEXTUAL};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let noir = Keyword {
            id: Some(6149),
            name: "neo-noir".into(),
        };
        let mut obs = observations_from_film(
            "Pulp Fiction",
            4.0,
            Some(1),
            &interaction_signal(4.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(1994),
            Some(154),
        );
        obs.extend(observations_from_film(
            "Drive",
            4.0,
            Some(2),
            &interaction_signal(4.0, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(2011),
            Some(100),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("drama{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let noir_aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "neo-noir")
            .expect("neo-noir");
        assert!(noir_aff.citeable());
        assert!(
            noir_aff.recommendation_mean < PORTABLE_CONTEXTUAL,
            "fixture must sit below the actor floor, got rec_mean={}",
            noir_aff.recommendation_mean
        );
        let mut cand = watchlist_drama("Fight Club", 550);
        cand.keywords = vec![noir];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "citeable neo-noir must not be dropped by an affinity floor, reasons={:?}",
            scored.reasons
        );
        assert_eq!(score_all(&profile, &[cand]).len(), 1);
    }

    #[test]
    fn watchlist_weak_actor_is_contextual() {
        let ((profile, _), actor) = drama_fraser_and_weak_actor();
        let mut cand = watchlist_drama("Gatto", 501);
        cand.credits = vec![actor];
        let scored = score_candidate(&profile, &cand);
        assert!(
            scored.contextual_only,
            "watchlist + weak actor must not occupy recommendation real estate, reasons={:?}",
            scored.reasons
        );
        assert!(score_all(&profile, &[cand]).is_empty());
    }

    #[test]
    fn watchlist_weak_actor_cannot_dominate_shortlist() {
        use crate::taste::shortlist::shortlist;
        let ((profile, dp), actor) = drama_fraser_and_weak_actor();
        let mut cands = Vec::new();
        for i in 0..20i64 {
            let mut c = watchlist_drama(&format!("Shelf Actor {i}"), 500 + i);
            c.credits = vec![actor.clone()];
            cands.push(c);
        }
        cands.push({
            let mut c = watchlist_drama("The Gambler", 77);
            c.credits = vec![dp.clone()];
            c
        });
        cands.push(Candidate {
            tmdb_id: Some(78),
            title: "Dune".into(),
            year: Some(2021),
            poster: None,
            genres: vec!["Science Fiction".into()],
            credits: vec![dp],
            keywords: vec![],
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
        });
        let scored = score_all(&profile, &cands);
        let short = shortlist(&scored);
        let weak = short
            .iter()
            .filter(|c| c.candidate.title.starts_with("Shelf Actor"))
            .count();
        assert_eq!(
            weak, 0,
            "watchlist+weak-actor must not occupy the shortlist, got {weak} of {}",
            short.len()
        );
        assert!(
            scored
                .iter()
                .any(|c| c.candidate.title == "The Gambler" && !c.contextual_only),
            "watchlist + Fraser must remain eligible"
        );
    }

    #[test]
    fn watchlist_with_signal_keyword_remains_eligible() {
        use crate::taste::features::Keyword;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let noir = Keyword {
            id: Some(6149),
            name: "neo-noir".into(),
        };
        let mut obs = observations_from_film(
            "Pulp Fiction",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(1994),
            Some(154),
        );
        obs.extend(observations_from_film(
            "Drive",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(2011),
            Some(100),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("drama{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let noir_aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "neo-noir")
            .expect("neo-noir");
        assert!(noir_aff.citeable());
        let mut cand = watchlist_drama("Fight Club", 550);
        cand.keywords = vec![noir];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "watchlist + signal keyword must remain eligible, reasons={:?}",
            scored.reasons
        );
        assert_eq!(score_all(&profile, &[cand]).len(), 1);
    }

    /// Real 627-film run after the watchlist fix: 12/50 were non-watchlist
    /// related films whose only cited evidence was Drama/Comedy/Crime/etc.
    #[test]
    fn reasons_do_not_treat_family_keyword_as_genre() {
        assert!(super::reasons_have_strong_bridge(&[
            "dysfunctional family affinity (0.69)".into()
        ]));
        assert!(!super::reasons_have_strong_bridge(&[
            "Drama affinity (0.32)".into(),
            "Comedy affinity (0.13)".into()
        ]));
    }

    #[test]
    fn related_genre_only_cannot_dominate_shortlist() {
        use crate::taste::shortlist::shortlist;
        let (profile, dp) = drama_and_fraser_profile();
        let shelf: Vec<(&str, Vec<&str>)> = vec![
            ("A Late Quartet", vec!["Drama", "Music"]),
            ("Disaster Holiday", vec!["Family", "Comedy"]),
            ("King of New York", vec!["Crime", "Thriller"]),
            ("The Blackcoat's Daughter", vec!["Mystery"]),
            ("Winged Creatures", vec!["Drama", "Crime"]),
            ("The Best of Me", vec!["Drama", "Romance"]),
            ("The Prodigy", vec!["Thriller"]),
            ("House of 1000 Corpses", vec!["Horror"]),
            ("Fatherhood", vec!["Drama", "Comedy"]),
            ("Chip 'n Dale: Rescue Rangers", vec!["Mystery", "Family", "Comedy"]),
            ("Rise of the Footsoldier", vec!["Crime", "Thriller"]),
            ("The Crush", vec!["Drama", "Thriller"]),
        ];
        let mut cands: Vec<_> = shelf
            .iter()
            .enumerate()
            .map(|(i, (title, genres))| related_genre(title, 700 + i as i64, genres))
            .collect();
        cands.push({
            let mut c = related_genre("Dune", 78, &["Science Fiction", "Drama"]);
            c.credits = vec![dp.clone()];
            c
        });
        let scored = score_all(&profile, &cands);
        let short = shortlist(&scored);
        let weak = short
            .iter()
            .filter(|c| shelf.iter().any(|(title, _)| *title == c.candidate.title))
            .count();
        assert_eq!(
            weak, 0,
            "related+genre-only must not occupy the shortlist, got {weak} of {}",
            short.len()
        );
        assert!(
            scored
                .iter()
                .any(|c| c.candidate.title == "Dune" && !c.contextual_only),
            "related + Fraser must remain eligible"
        );
    }

    #[test]
    fn related_with_craft_remains_eligible() {
        let (profile, dp) = drama_and_fraser_profile();
        let mut cand = related_genre("Dune", 78, &["Science Fiction", "Drama"]);
        cand.credits = vec![dp];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "related + Fraser is a recommendation signal, reasons={:?}",
            scored.reasons
        );
        assert_eq!(score_all(&profile, &[cand]).len(), 1);
    }

    #[test]
    fn related_with_signal_keyword_remains_eligible() {
        use crate::taste::features::Keyword;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let noir = Keyword {
            id: Some(6149),
            name: "neo-noir".into(),
        };
        let mut obs = observations_from_film(
            "Pulp Fiction",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(1994),
            Some(154),
        );
        obs.extend(observations_from_film(
            "Drive",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(2011),
            Some(100),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("drama{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let mut cand = related_genre("Motherless Brooklyn", 79, &["Crime", "Drama"]);
        cand.keywords = vec![noir];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "related + neo-noir must remain eligible, reasons={:?}",
            scored.reasons
        );
        assert_eq!(score_all(&profile, &[cand]).len(), 1);
    }

    #[test]
    fn related_thin_fraser_remains_eligible() {
        use crate::taste::features::PORTABLE_CONTEXTUAL;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let dp = Credit {
            id: Some(77),
            name: "Greig Fraser".into(),
            job: "Director of Photography".into(),
        };
        let mut obs = observations_from_film(
            "The Batman",
            4.0,
            Some(1),
            &interaction_signal(4.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2022),
            Some(176),
        );
        obs.extend(observations_from_film(
            "Rogue One: A Star Wars Story",
            4.0,
            Some(2),
            &interaction_signal(4.0, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2016),
            Some(133),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("drama{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let fraser = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .expect("Fraser");
        assert!(fraser.citeable());
        assert!(fraser.recommendation_mean < PORTABLE_CONTEXTUAL);
        let mut cand = related_genre("Dune", 78, &["Science Fiction", "Drama"]);
        cand.credits = vec![dp];
        let scored = score_candidate(&profile, &cand);
        assert!(
            !scored.contextual_only,
            "related + thin Fraser must stay eligible, rec_mean={}",
            fraser.recommendation_mean
        );
    }

    /// Real 627-film run: Disaster Holiday / Standing Up, Falling Down cited
    /// Curious George as Family/Comedy evidence despite sharing no people.
    #[test]
    fn comedy_candidate_does_not_use_curious_george_as_universal_evidence() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let mut obs = observations_from_film(
            "Curious George",
            5.0,
            Some(10),
            &interaction_signal(5.0, &p, Some(2.0), 1, false),
            Some(2.0),
            &["Family".into(), "Comedy".into(), "Animation".into()],
            &[],
            &[],
            Some(2006),
            Some(87),
        );
        obs.extend(observations_from_film(
            "Tony",
            4.5,
            Some(11),
            &interaction_signal(4.5, &p, Some(1.0), 1, false),
            Some(1.0),
            &["Comedy".into()],
            &[],
            &[],
            Some(2009),
            Some(90),
        ));
        for i in 0..6i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("pad{i}"),
                4.5,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Action".into()],
                &[],
                &[],
                Some(2019),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let cand = Candidate {
            tmdb_id: Some(88),
            title: "Disaster Holiday".into(),
            year: Some(2024),
            poster: None,
            genres: vec!["Family".into(), "Comedy".into()],
            credits: vec![],
            keywords: vec![],
            runtime: Some(90),
            vote_count: Some(200),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar to kids".into(),
                seed_tmdb_id: Some(10),
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.5,
        };
        let scored = score_candidate(&profile, &cand);
        assert!(
            scored
                .evidence
                .iter()
                .all(|e| !e.to_lowercase().contains("curious george")),
            "Curious George must not be a universal comedy/family seed, got {:?}",
            scored.evidence
        );
    }

    /// Real 627-film run: Better Off Dead ranked on duringcreditsstinger /
    /// aftercreditsstinger with Twilight/SpongeBob/Batman as evidence.
    #[test]
    fn stinger_keywords_do_not_become_recommendation_engines() {
        use crate::taste::features::Keyword;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let stinger = Keyword {
            id: Some(179431),
            name: "duringcreditsstinger".into(),
        };
        let mut obs = observations_from_film(
            "Spider-Man: No Way Home",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Action".into()],
            &[],
            &[stinger.clone()],
            Some(2021),
            Some(148),
        );
        obs.extend(observations_from_film(
            "The Batman",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.2), 1, false),
            Some(0.2),
            &["Crime".into()],
            &[],
            &[stinger.clone()],
            Some(2022),
            Some(176),
        ));
        obs.extend(observations_from_film(
            "Twilight",
            4.5,
            Some(3),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Drama".into()],
            &[],
            &[stinger.clone()],
            Some(2008),
            Some(122),
        ));
        let profile = build_profile(&obs);
        let cand = Candidate {
            tmdb_id: Some(4),
            title: "Better Off Dead...".into(),
            year: Some(1985),
            poster: None,
            genres: vec!["Comedy".into()],
            credits: vec![],
            keywords: vec![stinger],
            runtime: Some(97),
            vote_count: Some(800),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar".into(),
                seed_tmdb_id: Some(1),
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.5,
        };
        let scored = score_candidate(&profile, &cand);
        assert!(
            scored
                .reasons
                .iter()
                .all(|r| !r.to_lowercase().contains("stinger")),
            "TMDB stinger keywords must not drive ranking, got {:?}",
            scored.reasons
        );
        assert!(
            scored.evidence.iter().all(|e| !e.to_lowercase().contains("twilight")),
            "stinger overlap must not attach Twilight, got {:?}",
            scored.evidence
        );
    }

    /// Real 627-film run after genre-evidence scoping: 5 remaining Twilight
    /// citations were all "based on novel or book" / YA-novel catalog tags.
    #[test]
    fn based_on_novel_keyword_does_not_cite_twilight_on_every_adaptation() {
        use crate::taste::features::Keyword;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let novel = Keyword {
            id: Some(818),
            name: "based on novel or book".into(),
        };
        let mut obs = observations_from_film(
            "The Maze Runner",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into()],
            &[],
            &[novel.clone()],
            Some(2014),
            Some(113),
        );
        obs.extend(observations_from_film(
            "Twilight",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Drama".into()],
            &[],
            &[novel.clone()],
            Some(2008),
            Some(122),
        ));
        let profile = build_profile(&obs);
        let cand = Candidate {
            tmdb_id: Some(3),
            title: "Winged Creatures".into(),
            year: Some(2009),
            poster: None,
            genres: vec!["Drama".into()],
            credits: vec![],
            keywords: vec![novel],
            runtime: Some(100),
            vote_count: Some(400),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar".into(),
                seed_tmdb_id: Some(1),
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.5,
        };
        let scored = score_candidate(&profile, &cand);
        assert!(
            scored
                .reasons
                .iter()
                .all(|r| !r.to_lowercase().contains("based on novel")),
            "adaptation catalog tags must not drive ranking, got {:?}",
            scored.reasons
        );
        assert!(
            scored.evidence.iter().all(|e| !e.to_lowercase().contains("twilight")),
            "novel-tag overlap must not attach Twilight, got {:?}",
            scored.evidence
        );
    }

    fn score_with_keyword(
        kw_name: &str,
        candidate_title: &str,
    ) -> (
        crate::taste::features::FeatureProfile,
        ScoredCandidate,
    ) {
        use crate::taste::features::Keyword;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let kw = Keyword {
            id: Some(7),
            name: kw_name.into(),
        };
        let mut obs = observations_from_film(
            "Liked A",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Drama".into()],
            &[],
            &[kw.clone()],
            Some(2018),
            None,
        );
        obs.extend(observations_from_film(
            "Liked B",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Drama".into()],
            &[],
            &[kw.clone()],
            Some(2019),
            None,
        ));
        let profile = build_profile(&obs);
        let cand = Candidate {
            tmdb_id: Some(9),
            title: candidate_title.into(),
            year: Some(2009),
            poster: None,
            genres: vec!["Comedy".into()],
            credits: vec![],
            keywords: vec![kw],
            runtime: Some(100),
            vote_count: Some(400),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar".into(),
                seed_tmdb_id: Some(1),
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.5,
        };
        let scored = score_candidate(&profile, &cand);
        (profile, scored)
    }

    #[test]
    fn location_keyword_does_not_become_primary_recommendation_evidence() {
        let (profile, scored) = score_with_keyword("new york city", "Fame");
        let aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "new york city")
            .expect("location stays in the profile");
        assert!(!aff.citeable());
        assert!(
            scored
                .reasons
                .iter()
                .all(|r| !r.to_lowercase().contains("new york city")),
            "location must not be primary evidence, got {:?}",
            scored.reasons
        );
        assert!(
            scored
                .positive_features
                .iter()
                .all(|f| !f.eq_ignore_ascii_case("new york city"))
        );
    }

    #[test]
    fn cartoon_keyword_does_not_become_recommendation_engine() {
        let (_, scored) = score_with_keyword("cartoon", "Chip 'n Dale: Rescue Rangers");
        assert!(
            scored
                .reasons
                .iter()
                .all(|r| !r.to_lowercase().contains("cartoon")),
            "got {:?}",
            scored.reasons
        );
    }

    #[test]
    fn anti_hero_keyword_does_not_become_recommendation_engine() {
        let (_, scored) = score_with_keyword("anti hero", "The Public Enemy");
        assert!(
            scored
                .reasons
                .iter()
                .all(|r| !r.to_lowercase().contains("anti hero")),
            "got {:?}",
            scored.reasons
        );
    }

    #[test]
    fn neo_noir_keyword_remains_usable() {
        let (profile, scored) = score_with_keyword("neo-noir", "Motherless Brooklyn");
        let aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "neo-noir")
            .unwrap();
        assert!(aff.citeable());
        assert!(
            scored.reasons.iter().any(|r| r.to_lowercase().contains("neo-noir")),
            "got {:?}",
            scored.reasons
        );
    }

    #[test]
    fn long_take_keyword_remains_usable() {
        let (profile, scored) = score_with_keyword("long take", "Birdman");
        let aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "long take")
            .unwrap();
        assert!(aff.citeable());
        assert!(
            scored.reasons.iter().any(|r| r.to_lowercase().contains("long take")),
            "got {:?}",
            scored.reasons
        );
    }

    #[test]
    fn specific_keyword_with_repeated_evidence_remains_usable() {
        let (profile, scored) = score_with_keyword("heist", "Heat");
        let aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "heist")
            .unwrap();
        assert!(aff.citeable());
        assert!(
            scored.reasons.iter().any(|r| r.to_lowercase().contains("heist")),
            "got {:?}",
            scored.reasons
        );
        assert!(scored.evidence.iter().any(|e| e == "Liked A" || e == "Liked B"));
    }

    #[test]
    fn decade_only_candidates_are_dropped_from_pool() {
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
        use crate::taste::retrieve::{Candidate, RetrievalKind, RetrievalSource};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let genres = ["Drama", "Thriller", "Comedy", "Action", "Horror"];
        let mut obs = Vec::new();
        for i in 0..12i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("new{i}"),
                4.5,
                Some(i),
                &s,
                Some(0.4),
                &[genres[i as usize % 5].into()],
                &[],
                &[],
                Some(2005),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let dirty = Candidate {
            tmdb_id: Some(200),
            title: "Dirty".into(),
            year: Some(2005),
            poster: None,
            genres: vec!["Crime".into()],
            credits: vec![],
            keywords: vec![],
            runtime: None,
            vote_count: Some(10),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar to Twilight".into(),
                seed_tmdb_id: Some(2),
            }],
            friend_affinity: 0.0,
            tmdb_related: 1.0,
        };
        let scored = score_candidate(&profile, &dirty);
        assert!(scored.contextual_only);
        assert!(scored.reasons.iter().all(|r| !r.contains("2000s")));
        let pool = score_all(&profile, &[dirty]);
        assert!(pool.is_empty());
    }

    fn composer(name: &str, id: i64) -> Credit {
        Credit {
            id: Some(id),
            name: name.into(),
            job: "Original Music Composer".into(),
        }
    }

    fn watchlist_person(
        title: &str,
        tmdb_id: i64,
        genres: &[&str],
        person: Credit,
    ) -> Candidate {
        Candidate {
            tmdb_id: Some(tmdb_id),
            title: title.into(),
            year: Some(2014),
            poster: None,
            genres: genres.iter().map(|g| (*g).to_string()).collect(),
            credits: vec![person],
            keywords: vec![],
            runtime: Some(120),
            vote_count: Some(1000),
            watchlist: true,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Watchlist,
                label: "watchlist".into(),
                seed_tmdb_id: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        }
    }

    /// Real 627-film run: Fiore evidence Real Steel / Spider-Man, candidate
    /// Schindler's List. Same DP, no cluster overlap.
    #[test]
    fn fiore_schindlers_list_fails_cluster_without_other_signal() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let dp = Credit {
            id: Some(101),
            name: "Mauro Fiore".into(),
            job: "Director of Photography".into(),
        };
        let mut obs = observations_from_film(
            "Real Steel",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Action".into(), "Science Fiction".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2011),
            Some(127),
        );
        obs.extend(observations_from_film(
            "Spider-Man: No Way Home",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Action".into(), "Science Fiction".into(), "Adventure".into()],
            &[dp.clone()],
            &[],
            Some(2021),
            Some(148),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.0, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("other{i}"),
                4.0,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let fiore = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Mauro Fiore")
            .expect("Fiore");
        assert!(fiore.citeable());
        assert!(
            !fiore.evidence_cluster.is_empty(),
            "Action/Sci-Fi majority should form a cluster, got {:?}",
            fiore.evidence_cluster
        );

        let mut schindler = watchlist_person(
            "Schindler's List",
            424,
            &["Drama", "History", "War"],
            dp.clone(),
        );
        schindler.sources.push(RetrievalSource {
            kind: RetrievalKind::Filmography,
            label: "Mauro Fiore".into(),
            seed_tmdb_id: None,
        });
        assert!(
            !filmography_supported(&profile, &schindler),
            "watchlist must not bypass a failed Fiore cluster"
        );
        let scored = score_candidate(&profile, &schindler);
        assert!(
            scored.contextual_only,
            "Schindler's List must not inherit Real Steel/Spider-Man, reasons={:?}",
            scored.reasons
        );
        assert!(score_all(&profile, &[schindler]).is_empty());
    }

    /// Same Fiore/Schindler miss, but a citeable signal keyword on the
    /// candidate is an independent portable path.
    #[test]
    fn fiore_schindler_survives_with_independent_signal_keyword() {
        use crate::taste::features::Keyword;
        let p = rating_profile(&[4.0; 8]).unwrap();
        let dp = Credit {
            id: Some(101),
            name: "Mauro Fiore".into(),
            job: "Director of Photography".into(),
        };
        let noir = Keyword {
            id: Some(6149),
            name: "neo-noir".into(),
        };
        let mut obs = observations_from_film(
            "Real Steel",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Action".into(), "Science Fiction".into(), "Drama".into()],
            &[dp.clone()],
            &[],
            Some(2011),
            Some(127),
        );
        obs.extend(observations_from_film(
            "Spider-Man: No Way Home",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Action".into(), "Science Fiction".into(), "Adventure".into()],
            &[dp.clone()],
            &[],
            Some(2021),
            Some(148),
        ));
        obs.extend(observations_from_film(
            "Drive",
            4.5,
            Some(3),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(2011),
            Some(100),
        ));
        obs.extend(observations_from_film(
            "Pulp Fiction",
            4.5,
            Some(4),
            &interaction_signal(4.5, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into(), "Drama".into()],
            &[],
            &[noir.clone()],
            Some(1994),
            Some(154),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.0, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("other{i}"),
                4.0,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Comedy".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let mut schindler = watchlist_person(
            "Schindler's List",
            424,
            &["Drama", "History", "War"],
            dp,
        );
        schindler.keywords = vec![noir];
        schindler.sources.push(RetrievalSource {
            kind: RetrievalKind::Filmography,
            label: "Mauro Fiore".into(),
            seed_tmdb_id: None,
        });
        assert!(
            filmography_supported(&profile, &schindler),
            "an independent neo-noir signal must rescue a Fiore cluster miss"
        );
        assert!(!score_candidate(&profile, &schindler).contextual_only);
    }

    /// Real 627-film run: Cameron evidence Avatar / Titanic, candidate Billie
    /// Eilish 3D concert film. Prolific-director catalog injection.
    #[test]
    fn cameron_billie_eilish_fails_without_cluster_or_loyalty_overlap() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let director = Credit {
            id: Some(2710),
            name: "James Cameron".into(),
            job: "Director".into(),
        };
        let mut obs = observations_from_film(
            "Avatar: Fire and Ash",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into(), "Action".into(), "Adventure".into()],
            &[director.clone()],
            &[],
            Some(2025),
            Some(192),
        );
        obs.extend(observations_from_film(
            "Titanic",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(2.0), 1, false),
            Some(2.0),
            &["Drama".into(), "Romance".into()],
            &[director.clone()],
            &[],
            Some(1997),
            Some(194),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.0, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("other{i}"),
                4.0,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Comedy".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let cand = watchlist_person(
            "Billie Eilish - Hit Me Hard and Soft: The Tour (Live in 3D)",
            9,
            &["Music", "Documentary"],
            director,
        );
        let scored = score_candidate(&profile, &cand);
        assert!(
            scored.contextual_only,
            "concert film must not inherit Avatar/Titanic, reasons={:?} cluster={:?}",
            scored.reasons,
            profile
                .affinities
                .iter()
                .find(|a| a.key.name == "James Cameron")
                .map(|a| &a.evidence_cluster)
        );
        assert!(score_all(&profile, &[cand]).is_empty());
    }

    /// Counterexample: repeated highly-rated Knight/Laika animation should
    /// keep a new Knight film even with thin candidate metadata.
    #[test]
    fn knight_laika_loyalty_keeps_new_knight_film() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let knight = Credit {
            id: Some(202),
            name: "Travis Knight".into(),
            job: "Director".into(),
        };
        let mut obs = observations_from_film(
            "Kubo and the Two Strings",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Animation".into(), "Family".into(), "Adventure".into()],
            &[knight.clone()],
            &[],
            Some(2016),
            Some(102),
        );
        obs.extend(observations_from_film(
            "Missing Link",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Animation".into(), "Family".into(), "Adventure".into()],
            &[knight.clone()],
            &[],
            Some(2019),
            Some(94),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.0, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("other{i}"),
                4.0,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Drama".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let knight_aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Travis Knight")
            .expect("Knight");
        assert!(knight_aff.citeable());
        assert!(!knight_aff.evidence_cluster.is_empty());

        let mut piranesi = watchlist_person("Piranesi", 3, &[], knight.clone());
        piranesi.year = None;
        let scored = score_candidate(&profile, &piranesi);
        assert!(
            !scored.contextual_only,
            "Laika/Knight loyalty must keep a new Knight film, reasons={:?}",
            scored.reasons
        );
        assert_eq!(score_all(&profile, &[piranesi]).len(), 1);

        let kubo_like = watchlist_person(
            "Wildwood",
            4,
            &["Animation", "Family", "Adventure"],
            knight,
        );
        assert!(!score_candidate(&profile, &kubo_like).contextual_only);
    }

    /// Empty TMDB metadata on a *filmography* hit is catalog expansion, not
    /// Laika-style loyalty. Loyalty is watchlist-only.
    #[test]
    fn filmography_unenriched_is_not_automatic_loyalty() {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let director = Credit {
            id: Some(2710),
            name: "James Cameron".into(),
            job: "Director".into(),
        };
        let mut obs = observations_from_film(
            "Avatar",
            4.5,
            Some(1),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into(), "Action".into(), "Adventure".into()],
            &[director.clone()],
            &[],
            Some(2009),
            Some(162),
        );
        obs.extend(observations_from_film(
            "Avatar: The Way of Water",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Science Fiction".into(), "Action".into(), "Adventure".into()],
            &[director.clone()],
            &[],
            Some(2022),
            Some(192),
        ));
        for i in 0..8i64 {
            let s = interaction_signal(4.0, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("other{i}"),
                4.0,
                Some(30 + i),
                &s,
                Some(0.4),
                &["Comedy".into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let untitled = filmography_candidate("Untitled Cameron", 99, &[], director.clone());
        assert!(
            !filmography_supported(&profile, &untitled),
            "unenriched filmography must not inherit Avatar loyalty"
        );
        let abyss = filmography_candidate(
            "The Abyss",
            100,
            &["Science Fiction", "Action", "Adventure"],
            director,
        );
        assert!(
            filmography_supported(&profile, &abyss),
            "The Abyss shares the Avatar cluster and should transfer"
        );
    }

    fn filmography_candidate(
        title: &str,
        tmdb_id: i64,
        genres: &[&str],
        person: Credit,
    ) -> Candidate {
        let label = person.name.clone();
        Candidate {
            tmdb_id: Some(tmdb_id),
            title: title.into(),
            year: Some(2005),
            poster: None,
            genres: genres.iter().map(|g| (*g).to_string()).collect(),
            credits: vec![person],
            keywords: vec![],
            runtime: None,
            vote_count: Some(1000),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Filmography,
                label,
                seed_tmdb_id: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        }
    }

    fn powell_profile() -> (FeatureProfile, Credit) {
        let p = rating_profile(&[4.0; 8]).unwrap();
        let powell = composer("John Powell", 50);
        let mut obs = observations_from_film(
            "Kung Fu Panda",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(1.0), 1, false),
            Some(1.0),
            &["Comedy".into(), "Animation".into(), "Family".into()],
            &[powell.clone()],
            &[],
            Some(2008),
            None,
        );
        obs.extend(observations_from_film(
            "Minions & Monsters",
            5.0,
            Some(2),
            &interaction_signal(5.0, &p, Some(0.8), 1, false),
            Some(0.8),
            &["Comedy".into(), "Animation".into(), "Family".into()],
            &[powell.clone()],
            &[],
            Some(2010),
            None,
        ));
        let genres = [
            "Drama",
            "Thriller",
            "Comedy",
            "Action",
            "Horror",
            "Crime",
            "Mystery",
            "Science Fiction",
        ];
        for i in 0..16i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("log{i}"),
                4.5,
                Some(100 + i),
                &s,
                Some(0.4),
                &[genres[i as usize % genres.len()].into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        (build_profile(&obs), powell)
    }

    #[test]
    fn powell_filmography_cannot_dominate_shortlist() {
        use crate::taste::shortlist::shortlist;
        let (profile, powell) = powell_profile();
        let aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "John Powell")
            .unwrap();
        assert_eq!(aff.appearances, 2);
        assert!(aff.recommendation_mean > 0.5, "{}", aff.recommendation_mean);
        assert!(!aff.evidence_cluster.is_empty());

        let comedy = [
            "Ice Age: The Meltdown",
            "Antz",
            "Chicken Run",
            "Rio",
            "Horton Hears a Who!",
            "Robots",
            "Bolt",
            "Happy Feet",
        ];
        let unrelated = [
            ("The Bourne Supremacy", "Thriller"),
            ("United 93", "Drama"),
            ("Mr. & Mrs. Smith", "Action"),
            ("Be Cool", "Crime"),
            ("The Adventures of Pluto Nash", "Science Fiction"),
            ("Paycheck", "Thriller"),
            ("The Italian Job", "Action"),
            ("Hidalgo", "Adventure"),
            ("I Am Sam", "Drama"),
            ("Drumline", "Drama"),
            ("Two Weeks Notice", "Romance"),
            ("Stop-Loss", "Drama"),
        ];
        let mut candidates = Vec::new();
        for (i, title) in comedy.iter().enumerate() {
            candidates.push(filmography_candidate(
                title,
                200 + i as i64,
                &["Comedy", "Animation", "Family"],
                powell.clone(),
            ));
        }
        for (i, (title, genre)) in unrelated.iter().enumerate() {
            candidates.push(filmography_candidate(
                title,
                300 + i as i64,
                &[genre],
                powell.clone(),
            ));
        }
        for i in 0..60i64 {
            let g = ["Drama", "Thriller", "Comedy", "Crime", "Horror"][i as usize % 5];
            candidates.push(Candidate {
                tmdb_id: Some(400 + i),
                title: format!("Other {i}"),
                year: Some(2019),
                poster: None,
                genres: vec![g.into()],
                credits: vec![],
                keywords: vec![],
                runtime: None,
                vote_count: Some(200),
                watchlist: false,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Related,
                    label: "similar to log".into(),
                    seed_tmdb_id: Some(100),
                }],
                friend_affinity: 0.0,
                tmdb_related: 0.6,
            });
        }

        let scored = score_all(&profile, &candidates);
        assert!(
            scored.iter().all(|c| c.candidate.title != "United 93"),
            "drama-only Powell credit must not survive facet filter"
        );
        assert!(
            scored.iter().all(|c| c.candidate.title != "The Bourne Supremacy"),
            "thriller-only Powell credit must not survive facet filter"
        );
        assert!(
            scored
                .iter()
                .any(|c| c.candidate.title.contains("Ice Age") || c.candidate.title == "Antz"),
            "overlapping comedy/animation Powell credits should remain"
        );

        let short = shortlist(&scored);
        let powell_n = short
            .iter()
            .filter(|c| {
                c.positive_features
                    .iter()
                    .any(|f| f.contains("Powell"))
            })
            .count();
        assert!(
            powell_n < 8,
            "Person X must not take 8+ shortlist slots, got {powell_n} / {}",
            short.len()
        );

        let traces = person_pipeline_trace(&profile, &candidates, &scored, &short);
        let p = traces
            .iter()
            .find(|t| t.name == "John Powell")
            .unwrap();
        assert_eq!(p.injected, 20);
        assert_eq!(p.appearances, 2);
        assert!(
            p.survived_score < p.injected,
            "facet filter should drop unrelated filmography: score {} injected {}",
            p.survived_score,
            p.injected
        );
        assert!(p.survived_mmr < 8, "MMR {}", p.survived_mmr);
        assert!(p.confidence < 0.5, "n=2 composer k=4 → ~0.39, got {}", p.confidence);
        assert!(p.scoring_affinity < p.recommendation_mean);
    }

    #[test]
    fn fraser_filmography_is_not_suppressed_when_cluster_is_empty() {
        use crate::taste::shortlist::shortlist;
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
            None,
        );
        obs.extend(observations_from_film(
            "Dune",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Science Fiction".into()],
            &[dp.clone()],
            &[],
            Some(2021),
            None,
        ));
        for i in 0..12i64 {
            let s = interaction_signal(4.5, &p, Some(0.4), 1, false);
            obs.extend(observations_from_film(
                &format!("log{i}"),
                4.5,
                Some(50 + i),
                &s,
                Some(0.4),
                &[["Drama", "Comedy", "Thriller", "Horror"][i as usize % 4].into()],
                &[],
                &[],
                Some(2018),
                None,
            ));
        }
        let profile = build_profile(&obs);
        let mut candidates = vec![
            filmography_candidate("Zero Dark Thirty", 1, &["Thriller"], dp.clone()),
            filmography_candidate("Mary Magdalene", 2, &["Drama"], dp.clone()),
            filmography_candidate("Rogue One", 3, &["Science Fiction", "Action"], dp.clone()),
        ];
        for i in 0..20i64 {
            candidates.push(Candidate {
                tmdb_id: Some(80 + i),
                title: format!("Other {i}"),
                year: Some(2019),
                poster: None,
                genres: vec!["Drama".into()],
                credits: vec![],
                keywords: vec![],
                runtime: None,
                vote_count: Some(200),
                watchlist: false,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Related,
                    label: "similar".into(),
                    seed_tmdb_id: Some(50),
                }],
                friend_affinity: 0.0,
                tmdb_related: 0.5,
            });
        }
        let scored = score_all(&profile, &candidates);
        assert!(
            scored.iter().all(|c| c.candidate.title != "Zero Dark Thirty"),
            "empty cluster must not dump generic Fraser filmography"
        );
        assert!(
            scored.iter().all(|c| c.candidate.title != "Mary Magdalene"),
            "Fraser Crime/Sci-Fi evidence must not transfer to a Drama-only credit"
        );
        assert!(
            scored.iter().any(|c| c.candidate.title == "Rogue One"),
            "Rogue One shares Sci-Fi with Dune evidence"
        );
        let short = shortlist(&scored);
        let fraser_n = short
            .iter()
            .filter(|c| {
                c.positive_features
                    .iter()
                    .any(|f| f.contains("Fraser"))
            })
            .count();
        assert!(fraser_n >= 1, "Fraser should still matter");
        assert!(fraser_n < 8, "got {fraser_n}");
    }
}
