use crate::taste::retrieve::RetrievalKind;
use crate::taste::score::{evidence_grade, ScoredCandidate};
use crate::taste::features::{keyword_strength, KeywordStrength};
use chrono::Datelike;
use std::collections::HashSet;

pub const MATCH_SCORE_FLOOR: u8 = 50;
pub const NEW_MATCH_FLOOR: u8 = 70;
pub const RELATED_ONLY_CAP: u8 = 69;
pub const SINGLE_BRIDGE_CAP: u8 = 69;
pub const LIMITED_EVIDENCE_CAP: u8 = 69;
/// Temporary honesty cap: a strong unseen card cannot yet display 80%+.
/// Remove once match % is recalibrated independently of watchlist membership.
pub const NON_WATCHLIST_BAND_CAP: u8 = 79;
pub const EXCELLENT_BAND: u8 = 90;
const APPEARANCE_CAP: u32 = 8;
const LIMITED_PENALTY: f32 = 0.82;
const STRENGTH_SCALE: f32 = 0.38;
const STRENGTH_WEIGHT: f32 = 0.72;
const GRADE_WEIGHT: f32 = 0.18;
const APPEARANCE_WEIGHT: f32 = 0.10;
const PORTABLE_AFFINITY_FLOOR: f32 = 0.18;
const PORTABLE_APPEARANCES: u32 = 3;

const CRAFT: &[&str] = &[
    "director",
    "writer",
    "cinematographer",
    "composer",
    "actor",
];
pub fn match_score(c: &ScoredCandidate) -> u8 {
    let (quality, n) = best_craft_quality(c);
    let strength = (quality / STRENGTH_SCALE).tanh().clamp(0.0, 1.0);
    let g = (evidence_grade(c) as f32 / 3.0).clamp(0.0, 1.0);
    let appear = 1.0 - (-(n.min(APPEARANCE_CAP) as f32) / APPEARANCE_CAP as f32).exp();
    let limited = if is_limited_evidence(c) {
        LIMITED_PENALTY
    } else {
        1.0
    };
    let raw = ((STRENGTH_WEIGHT * strength + GRADE_WEIGHT * g + APPEARANCE_WEIGHT * appear)
        * limited)
        .clamp(0.0, 1.0);
    let mut score = (100.0 * raw).round().clamp(0.0, 100.0) as u8;
    score = score.max(recommendation_neighbor_floor(c));
    if related_only(c) {
        score = score.min(RELATED_ONLY_CAP);
    }
    if filmography_single_bridge(c) {
        score = score.min(SINGLE_BRIDGE_CAP);
    }
    if has_filmography_source(c)
        && c.candidate
            .sources
        .iter()
            .any(|s| s.kind.is_related())
        && !qualifying_director_dp_filmography(c)
        && !mixed_recommendation_corroboration(c)
        && !c.candidate.watchlist
    {
        score = score.min(SINGLE_BRIDGE_CAP);
    }
    if is_limited_evidence(c) {
        score = score.min(LIMITED_EVIDENCE_CAP);
    }
    if !c.candidate.watchlist && kids_or_animation(c) {
        score = score.min(RELATED_ONLY_CAP);
    }
    if !c.candidate.watchlist
        && has_filmography_source(c)
        && !has_new_corroboration(c)
        && !mixed_recommendation_corroboration(c)
    {
        score = score.min(RELATED_ONLY_CAP);
    }
    if !c.candidate.watchlist {
        score = score.min(NON_WATCHLIST_BAND_CAP);
    }
    if qualifying_director_dp_filmography(c) {
        score = score.max(NEW_MATCH_FLOOR);
    }
    score
}

pub fn passes_match_floor(c: &ScoredCandidate) -> bool {
    match_score(c) >= MATCH_SCORE_FLOOR
}

pub fn thin_evidence(c: &ScoredCandidate) -> bool {
    is_limited_evidence(c)
}

fn cited_craft(c: &ScoredCandidate) -> Vec<&crate::taste::explain::MatchedFeatureView> {
    c.matched_features
        .iter()
        .filter(|f| f.cited && CRAFT.contains(&f.family.as_str()))
        .collect()
}

fn best_craft_quality(c: &ScoredCandidate) -> (f32, u32) {
    cited_craft(c)
        .into_iter()
        .map(|f| {
            let quality = (f.scoring_affinity.max(0.0) * f.portability.clamp(0.0, 1.0)).max(0.0);
            (quality, f.appearances)
        })
        .max_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        })
        .unwrap_or((0.0, 0))
}

fn is_limited_evidence(c: &ScoredCandidate) -> bool {
    let people = cited_craft(c);
    if people.is_empty() {
        return true;
    }
    people.iter().all(|f| f.appearances <= 2)
}

pub fn related_only(c: &ScoredCandidate) -> bool {
    if c.candidate.watchlist || c.candidate.sources.is_empty() {
        return false;
    }
    if !c
        .candidate
        .sources
        .iter()
        .all(|s| s.kind.is_related())
    {
        return false;
    }
    // Neighbor retrieval is the point of relatedRecommendations. Similar-to
    // dumps stay capped at 69; corroborated recs are not treated as related-only.
    if has_recommendation_source(c) && has_new_corroboration(c) {
        return false;
    }
    true
}

fn has_recommendation_source(c: &ScoredCandidate) -> bool {
    c.candidate.sources.iter().any(|s| {
        s.kind == RetrievalKind::RelatedRecommendations
    })
}

fn has_direct_portable_director_or_dp(c: &ScoredCandidate) -> bool {
    cited_craft(c).into_iter().any(|f| {
        matches!(f.family.as_str(), "director" | "cinematographer")
            && f.scoring_affinity >= PORTABLE_AFFINITY_FLOOR
            && f.appearances >= PORTABLE_APPEARANCES
    })
}

fn kids_or_animation(c: &ScoredCandidate) -> bool {
    c.candidate.genres.iter().any(|g| {
        let lower = g.to_ascii_lowercase();
        lower == "animation" || lower == "family" || lower.contains("kid") || lower == "tv movie"
    })
}

fn tv_movie(c: &ScoredCandidate) -> bool {
    c.candidate
        .genres
        .iter()
        .any(|g| g.eq_ignore_ascii_case("tv movie"))
}

pub fn filmography_only(c: &ScoredCandidate) -> bool {
    if c.candidate.watchlist || c.candidate.sources.is_empty() {
        return false;
    }
    c.candidate
        .sources
        .iter()
        .all(|s| s.kind == RetrievalKind::Filmography)
}

fn qualified_bridge_count(c: &ScoredCandidate) -> usize {
    cited_craft(c)
        .into_iter()
        .filter(|f| {
            matches!(
                f.family.as_str(),
                "director" | "writer" | "cinematographer" | "composer"
            )
        })
        .count()
}

fn has_catalog_proof(c: &ScoredCandidate) -> bool {
    matches!(c.candidate.runtime, Some(rt) if rt >= crate::taste::score::FEATURE_RUNTIME_MIN)
        || matches!(c.candidate.vote_count, Some(n) if n >= 1)
}

fn incomplete_metadata_stub(c: &ScoredCandidate) -> bool {
    c.candidate.year.is_none() && !has_catalog_proof(c)
}

fn future_dated(c: &ScoredCandidate) -> bool {
    let year_now = chrono::Utc::now().year();
    matches!(c.candidate.year, Some(y) if y > year_now)
}

/// Future-dated or yearless stubs with no catalog proof stay off New and Explore.
/// Watchlist titles like Avatar 4 may still be future-dated.
pub fn unreleased_display_row(c: &ScoredCandidate) -> bool {
    if c.candidate.watchlist {
        return false;
    }
    future_dated(c) || incomplete_metadata_stub(c)
}

/// Year-less filmography stubs stay off New even when runtime/votes exist.
pub fn unreleased_new_row(c: &ScoredCandidate) -> bool {
    if c.candidate.watchlist {
        return false;
    }
    unreleased_display_row(c) || (c.candidate.year.is_none() && filmography_only(c))
}

fn qualifying_director_dp_filmography(c: &ScoredCandidate) -> bool {
    has_filmography_source(c)
        && has_direct_portable_director_or_dp(c)
        && c.eligibility.candidate_fit >= 0.999
        && !unreleased_new_row(c)
        && !tv_movie(c)
        && !kids_or_animation(c)
        && has_new_corroboration(c)
}

fn portable_cited_craft(c: &ScoredCandidate) -> Vec<&crate::taste::explain::MatchedFeatureView> {
    cited_craft(c)
        .into_iter()
        .filter(|f| {
            matches!(
                f.family.as_str(),
                "director" | "writer" | "cinematographer" | "composer"
            ) && f.appearances >= PORTABLE_APPEARANCES
                && f.scoring_affinity >= PORTABLE_AFFINITY_FLOOR
        })
        .collect()
}

fn has_strong_cited_keyword(c: &ScoredCandidate) -> bool {
    c.matched_features.iter().any(|f| {
        f.cited
            && f.family == "keyword"
            && keyword_strength(&f.name) == KeywordStrength::Strong
    })
}

/// New needs more than "this DP/director is on the crew." A second portable
/// craft person (or the same auteur as director+writer) or a strong craft
/// keyword like neo-noir is the corroboration.
fn has_new_corroboration(c: &ScoredCandidate) -> bool {
    let craft = portable_cited_craft(c);
    craft.len() >= 2 || (craft.len() >= 1 && has_strong_cited_keyword(c))
}

/// Several distinct loved recommendation seeds can corroborate a filmography
/// lead. Keep one-seed résumé expansion out, while allowing the strongest
/// multi-neighbor rows to backfill New when the direct craft bridge is sparse.
fn mixed_recommendation_corroboration(c: &ScoredCandidate) -> bool {
    if filmography_only(c) {
        return false;
    }
    let seeds = crate::taste::score::unique_loved_rec_seeds(c);
    seeds >= 3
        || (seeds >= 2
            && c.eligibility.evidence_grade.rank() >= 3
            && c.eligibility.candidate_fit >= 0.65)
}

pub fn filmography_single_bridge(c: &ScoredCandidate) -> bool {
    filmography_only(c)
        && qualified_bridge_count(c) < 2
        && !qualifying_director_dp_filmography(c)
}

fn has_filmography_source(c: &ScoredCandidate) -> bool {
    c.candidate
        .sources
        .iter()
        .any(|s| s.kind == RetrievalKind::Filmography)
}

/// TMDB recommendations from several loved films are a real neighbor signal
/// even when no DP/director is shared. One seed is not enough (Tony → Curves).
fn recommendation_neighbor_floor(c: &ScoredCandidate) -> u8 {
    if c.candidate.watchlist || kids_or_animation(c) {
        return 0;
    }
    let seeds: HashSet<i64> = c
        .candidate
        .sources
        .iter()
        .filter(|s| s.kind == RetrievalKind::RelatedRecommendations)
        .filter_map(|s| s.seed_tmdb_id)
        .collect();
    match seeds.len() {
        n if n >= 6 => 66,
        5 => 64,
        4 => 62,
        3 => 58,
        2 => 52,
        _ => 0,
    }
}

/// Résumé filmography still needs a director/DP or corroboration. Neighbors
/// of loved films may occupy New; the match percent is the honesty.
pub fn has_independent_new_bridge(c: &ScoredCandidate) -> bool {
    if has_filmography_source(c) {
        return qualifying_director_dp_filmography(c)
            || has_new_corroboration(c)
            || mixed_recommendation_corroboration(c);
    }
    true
}

pub fn occupies_new(c: &ScoredCandidate) -> bool {
    !c.candidate.watchlist
        && c.eligibility.evidence_grade.displayable()
        && !unreleased_new_row(c)
        && !tv_movie(c)
        && has_independent_new_bridge(c)
        && !filmography_single_bridge(c)
}

/// Explore is folded into New. Kept so older run logs still deserialize.
pub fn occupies_explore(_c: &ScoredCandidate) -> bool {
    false
}

pub fn weak_filmography_resume(c: &ScoredCandidate) -> bool {
    filmography_single_bridge(c)
}

/// Why a ranked row did not occupy New. Watchlist and New members are `None`.
pub fn filter_reason(c: &ScoredCandidate) -> Option<String> {
    let short_runtime = matches!(
        c.candidate.runtime,
        Some(rt) if (1..crate::taste::score::FEATURE_RUNTIME_MIN).contains(&rt)
    );
    if (c.candidate.watchlist && c.eligibility.passed && c.eligibility.evidence_grade.displayable())
        || (!c.candidate.watchlist && occupies_new(c))
    {
        return None;
    }
    if short_runtime {
        return Some("short-runtime".into());
    }
    if future_dated(c) {
        return Some("unreleased".into());
    }
    if incomplete_metadata_stub(c) {
        return Some("incomplete-metadata".into());
    }
    if unreleased_new_row(c) {
        return Some("unreleased".into());
    }
    if filmography_single_bridge(c) {
        return Some("filmography-only".into());
    }
    if !c.eligibility.passed || !c.eligibility.evidence_grade.displayable() {
        return Some("weak-evidence".into());
    }
    Some("held".into())
}

/// Eligibility placement. Displayed `section` in the run log is computed from
/// the assembled workspace and may be `held` when a cap omits a member.
pub fn placement(c: &ScoredCandidate) -> &'static str {
    if c.candidate.watchlist {
        "watchlist"
    } else if occupies_new(c) {
        "new"
    } else if occupies_explore(c) {
        "explore"
    } else {
        "held"
    }
}

pub fn sort_workspace(rows: &mut [ScoredCandidate]) {
    rows.sort_by(rank_order);
}

/// Internal ranking. Evidence grade chooses the quality band; fit and
/// corroboration order rows within that band.
pub fn rank_order(a: &ScoredCandidate, b: &ScoredCandidate) -> std::cmp::Ordering {
    b.eligibility
        .evidence_grade
        .rank()
        .cmp(&a.eligibility.evidence_grade.rank())
        .then_with(|| {
            b.eligibility
                .candidate_fit
                .partial_cmp(&a.eligibility.candidate_fit)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            crate::taste::score::unique_loved_rec_seeds(b)
                .cmp(&crate::taste::score::unique_loved_rec_seeds(a))
        })
        .then_with(|| {
            b.score
                .negative_evidence
                .partial_cmp(&a.score.negative_evidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            b.score
                .total
                .partial_cmp(&a.score.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            a.candidate
                .tmdb_id
                .unwrap_or(i64::MAX)
                .cmp(&b.candidate.tmdb_id.unwrap_or(i64::MAX))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::explain::{EvidenceGrade, MatchedFeatureView};
    use crate::taste::retrieve::{MediaKind, RetrievalKind, RetrievalSource};
    use crate::taste::score::{CandidateScore, CandidateView};

    fn feat(family: &str, appearances: u32, affinity: f32) -> MatchedFeatureView {
        MatchedFeatureView {
            feature_key: String::new(),
            name: format!("{family}-{appearances}"),
            family: family.into(),
            appearances,
            recommendation_mean: affinity,
            scoring_affinity: affinity,
            confidence: 0.8,
            portability: 1.0,
            citeable: true,
            cited: true,
        }
    }

    fn neo_noir() -> MatchedFeatureView {
        MatchedFeatureView {
            feature_key: String::new(),
            name: "neo-noir".into(),
            family: "keyword".into(),
            appearances: 7,
            recommendation_mean: 0.4,
            scoring_affinity: 0.4,
            confidence: 0.8,
            portability: 1.0,
            citeable: true,
            cited: true,
        }
    }

    fn row_with(
        features: Vec<MatchedFeatureView>,
        watchlist: bool,
        related: bool,
        total: f32,
        tmdb_id: i64,
    ) -> ScoredCandidate {
        let person_keys: Vec<_> = features.iter().map(|f| f.name.clone()).collect();
        let evidence_grade = if watchlist {
            if features.iter().any(|f| f.appearances >= 3) {
                EvidenceGrade::Strong
            } else {
                EvidenceGrade::Medium
            }
        } else {
            EvidenceGrade::Medium
        };
        ScoredCandidate {
            candidate: CandidateView {
                tmdb_id: Some(tmdb_id),
                title: "Film".into(),
                year: Some(2000),
                poster: None,
                watchlist,
                sources: vec![RetrievalSource {
                    kind: if related {
                        RetrievalKind::Related
                    } else if watchlist {
                        RetrievalKind::Watchlist
                    } else {
                        RetrievalKind::Filmography
                    },
                    label: "x".into(),
                    seed_tmdb_id: None,
                    seed_rating: None,
                }],
                directors: vec!["A".into()],
                genres: vec![],
                modes: vec![],
                media_kind: MediaKind::Movie,
                runtime: Some(110),
                vote_count: Some(400),
            },
            score: CandidateScore {
                content: 0.5,
                tmdb_related: 0.0,
                friend_affinity: 0.0,
                recent_taste: 0.0,
                watchlist: 0.0,
                novelty: 0.0,
                negative_evidence: 0.0,
                semantic_fit: 0.5,
                semantic_coverage: false,
                total,
            },
            reasons: vec![],
            evidence: vec![],
            positive_features: person_keys.clone(),
            negative_features: vec![],
            contextual_only: false,
            person_keys,
            display_reasons: vec![],
            scoring_reasons: vec![],
            matched_features: features,
            hidden_features: vec![],
            eligibility: crate::taste::explain::EligibilityTrace {
                portable_evidence_required: true,
                passed: true,
                passed_because: vec!["fixture".into()],
                candidate_fit: 1.0,
                evidence_grade,
            },
        }
    }

    fn row(grade_people: &[(u32, bool)], watchlist: bool) -> ScoredCandidate {
        let features: Vec<_> = grade_people
            .iter()
            .map(|(n, _)| feat("director", *n, 0.5))
            .collect();
        row_with(features, watchlist, false, 0.4, 1)
    }

    fn with_total(mut c: ScoredCandidate, total: f32) -> ScoredCandidate {
        c.score.total = total;
        c
    }

    #[test]
    fn higher_evidence_grade_scores_higher() {
        let g3 = row(&[(5, true), (4, true)], true);
        let g2 = row(&[(2, true)], false);
        assert!(match_score(&g3) > match_score(&g2));
    }

    #[test]
    fn more_appearances_only_raise_score_modestly() {
        let n8 = row_with(vec![feat("composer", 8, 0.22)], false, false, 0.4, 1);
        let n4 = row_with(vec![feat("composer", 4, 0.22)], false, false, 0.4, 2);
        let delta = match_score(&n8) as i16 - match_score(&n4) as i16;
        assert!(delta >= 0, "n=8 should not lose to n=4, delta={delta}");
        assert!(delta <= 12, "appearance count must not dominate, delta={delta}");
    }

    #[test]
    fn appearance_cap_treats_11_like_8() {
        let n8 = row(&[(8, true)], true);
        let n11 = row(&[(11, true)], true);
        assert_eq!(match_score(&n8), match_score(&n11));
    }

    #[test]
    fn limited_evidence_lowers_score() {
        let strong = row(&[(8, true)], true);
        let limited = row(&[(2, true)], true);
        assert!(match_score(&limited) < match_score(&strong));
    }

    #[test]
    fn low_evidence_can_fall_below_floor() {
        let weak = row_with(vec![], false, false, 0.05, 2);
        assert!(match_score(&weak) < MATCH_SCORE_FLOOR);
        assert!(!passes_match_floor(&weak));
    }

    #[test]
    fn frozen_score_is_sort_tiebreak_only() {
        let a = with_total(row(&[(8, true)], true), 0.1);
        let b = with_total(row(&[(8, true)], true), 0.9);
        assert_eq!(match_score(&a), match_score(&b));
    }

    #[test]
    fn same_evidence_same_score_across_pool_totals() {
        let a = with_total(row(&[(5, true)], true), -0.2);
        let b = with_total(row(&[(5, true)], true), 1.2);
        assert_eq!(match_score(&a), match_score(&b));
    }

    #[test]
    fn stronger_affinity_outranks_busier_collaborator() {
        let fraser = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.01,
            20,
        );
        let zimmer = row_with(vec![feat("composer", 12, 0.22)], false, true, 0.9, 10);
        assert!(match_score(&fraser) > match_score(&zimmer));
        assert!(match_score(&zimmer) < EXCELLENT_BAND);
    }

    #[test]
    fn composer_count_cannot_make_related_boss_baby_excellent() {
        let boss_baby = row_with(
            vec![
                feat("composer", 12, 0.22),
                feat("actor", 2, 0.12),
            ],
            false,
            true,
            0.8,
            459_151,
        );
        let if_movie = row_with(
            vec![
                feat("composer", 9, 0.24),
                feat("actor", 3, 0.14),
            ],
            false,
            true,
            0.8,
            1,
        );
        let star_trek = row_with(
            vec![
                feat("composer", 9, 0.24),
                feat("actor", 5, 0.16),
            ],
            false,
            true,
            0.8,
            2,
        );
        for row in [&boss_baby, &if_movie, &star_trek] {
            assert!(
                match_score(row) < EXCELLENT_BAND,
                "{} scored {} Excellent from collaborator frequency",
                row.candidate.tmdb_id.unwrap(),
                match_score(row)
            );
            assert!(
                match_score(row) <= RELATED_ONLY_CAP,
                "related-only without a DP/director/writer must stay a discovery"
            );
        }
        let foxcatcher = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.2,
            3,
        );
        assert!(match_score(&foxcatcher) > match_score(&boss_baby));
        assert!(match_score(&foxcatcher) > match_score(&if_movie));
        assert!(match_score(&foxcatcher) > match_score(&star_trek));
    }

    #[test]
    fn related_only_is_capped_even_with_portable_dp() {
        let dune = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            true,
            0.5,
            78,
        );
        assert!(
            match_score(&dune) <= RELATED_ONLY_CAP,
            "related-only must never pad into Strong possibility, got {}",
            match_score(&dune)
        );
        assert!(
            occupies_new(&dune) || match_score(&dune) < MATCH_SCORE_FLOOR,
            "a portable DP neighbor belongs on New, not a hidden Explore shelf"
        );
        assert!(!occupies_explore(&dune));
    }

    #[test]
    fn displayed_order_is_fit_then_total_then_tmdb() {
        let mut low = with_total(row(&[(8, true)], false), 0.01);
        low.eligibility.candidate_fit = 0.4;
        low.candidate.tmdb_id = Some(10);
        let mut high = with_total(row(&[(8, true)], false), 0.99);
        high.eligibility.candidate_fit = 1.0;
        high.candidate.tmdb_id = Some(20);
        let mut rows = vec![low, high];
        sort_workspace(&mut rows);
        assert_eq!(rows[0].candidate.tmdb_id, Some(20));
    }

    #[test]
    fn filmography_single_bridge_caps_as_discovery() {
        let be_cool = row_with(vec![feat("composer", 8, 0.55)], false, false, 0.4, 70);
        assert!(
            match_score(&be_cool) <= SINGLE_BRIDGE_CAP,
            "single-person filmography must not read as Strong possibility, got {}",
            match_score(&be_cool)
        );
        let mut two_bridges = row_with(
            vec![
                feat("cinematographer", 5, 0.41),
                feat("cinematographer", 8, 0.38),
            ],
            false,
            false,
            0.4,
            71,
        );
        two_bridges.matched_features[0].name = "Mauro Fiore".into();
        two_bridges.matched_features[1].name = "Wally Pfister".into();
        assert!(
            match_score(&two_bridges) > SINGLE_BRIDGE_CAP,
            "two craft people on the same film may outrank Discovery, got {}",
            match_score(&two_bridges)
        );
    }

    #[test]
    fn limited_evidence_cannot_display_strong_possibility() {
        let thin = row_with(vec![feat("actor", 2, 0.7)], false, false, 0.5, 80);
        assert!(thin_evidence(&thin));
        assert!(
            match_score(&thin) <= LIMITED_EVIDENCE_CAP,
            "thin evidence must stay below Strong possibility, got {}",
            match_score(&thin)
        );
    }

    #[test]
    fn candidate_fit_ranks_without_changing_visible_match_score() {
        let mut weak = row_with(vec![feat("composer", 8, 0.55)], false, false, 0.4, 81);
        weak.eligibility.candidate_fit = 0.32;
        let mut specific = weak.clone();
        specific.eligibility.candidate_fit = 1.0;
        specific.candidate.tmdb_id = Some(82);
        assert_eq!(
            match_score(&weak),
            match_score(&specific),
            "candidate fit is an internal rank component, not visible match percent"
        );
        assert!(match_score(&weak) <= SINGLE_BRIDGE_CAP);
    }

    #[test]
    fn watchlist_match_score_ignores_candidate_fit() {
        let mut prestige = row_with(
            vec![feat("cinematographer", 3, 0.39), feat("director", 5, 0.55)],
            true,
            false,
            0.4,
            1124,
        );
        prestige.eligibility.candidate_fit = 0.55;
        let mut full = prestige.clone();
        full.eligibility.candidate_fit = 1.0;
        assert_eq!(
            match_score(&prestige),
            match_score(&full),
            "Watchlist Nolan/Pfister titles must not lose a band to candidate_fit"
        );
        assert!(
            match_score(&prestige) >= NEW_MATCH_FLOOR,
            "Prestige-class watchlist must stay Strong possibility, got {}",
            match_score(&prestige)
        );
    }

    fn related_seed(
        features: Vec<MatchedFeatureView>,
        genres: &[&str],
        fit: f32,
        tmdb_id: i64,
    ) -> ScoredCandidate {
        let mut c = row_with(features, false, true, 0.5, tmdb_id);
        c.candidate.sources[0].seed_tmdb_id = Some(27_205);
        c.candidate.genres = genres.iter().map(|g| (*g).to_string()).collect();
        c.eligibility.candidate_fit = fit;
        c
    }

    #[test]
    fn portable_filmography_with_specific_fit_occupies_new() {
        let mut kts = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.4,
            49_530,
        );
        kts.eligibility.candidate_fit = 1.0;
        kts.matched_features.push(neo_noir());
        assert!(
            match_score(&kts) >= NEW_MATCH_FLOOR,
            "Fraser + specific fit must clear Strong possibility, got {}",
            match_score(&kts)
        );
        assert!(occupies_new(&kts), "Killing Them Softly belongs on New");
        assert_eq!(filter_reason(&kts), None);
    }

    #[test]
    fn composer_filmography_stays_discovery_even_with_specific_fit() {
        let mut antz = row_with(vec![feat("composer", 9, 0.55)], false, false, 0.4, 101);
        antz.eligibility.candidate_fit = 1.0;
        assert!(
            match_score(&antz) <= SINGLE_BRIDGE_CAP,
            "Powell résumé cards must stay Discovery, got {}",
            match_score(&antz)
        );
        assert!(!occupies_new(&antz));
        assert_eq!(filter_reason(&antz).as_deref(), Some("filmography-only"));
    }

    #[test]
    fn loved_similar_without_craft_does_not_occupy_new() {
        let mut raging = related_seed(vec![], &["Drama"], 1.0, 11);
        let mut kissing = related_seed(vec![], &["Romance", "Comedy"], 1.0, 15);
        raging.eligibility.evidence_grade = EvidenceGrade::None;
        kissing.eligibility.evidence_grade = EvidenceGrade::None;
        raging.eligibility.passed = false;
        kissing.eligibility.passed = false;
        for row in [&raging, &kissing] {
            assert!(
                match_score(row) < NEW_MATCH_FLOOR,
                "TMDB similar-to must not be padded to Strong possibility, got {} for {}",
                match_score(row),
                row.candidate.tmdb_id.unwrap()
            );
            assert!(!occupies_new(row));
        }
        assert_eq!(filter_reason(&kissing).as_deref(), Some("weak-evidence"));
    }

    #[test]
    fn modest_portable_filmography_still_occupies_new() {
        let mut insomnia = row_with(
            vec![feat("cinematographer", 3, 0.32)],
            false,
            false,
            0.4,
            320,
        );
        insomnia.eligibility.candidate_fit = 1.0;
        insomnia.matched_features.push(feat("director", 5, 0.38));
        insomnia.matched_features.push(neo_noir());
        assert!(
            match_score(&insomnia) >= NEW_MATCH_FLOOR,
            "Pfister/Fiore filmography with a specific match must not stall at Discovery, got {}",
            match_score(&insomnia)
        );
        assert!(occupies_new(&insomnia));
    }

    #[test]
    fn animation_similar_and_composer_related_stay_off_new() {
        let mut feet = related_seed(
            vec![feat("composer", 9, 0.55)],
            &["Animation", "Family"],
            1.0,
            12,
        );
        feet.eligibility.evidence_grade = EvidenceGrade::None;
        let trek = related_seed(
            vec![feat("composer", 12, 0.24)],
            &["Science Fiction", "Action"],
            1.0,
            13,
        );
        let mut sponge = related_seed(
            vec![feat("writer", 3, 0.4)],
            &["Animation"],
            1.0,
            14,
        );
        sponge.eligibility.evidence_grade = EvidenceGrade::None;
        for row in [&feet, &sponge] {
            assert!(
                match_score(row) <= RELATED_ONLY_CAP,
                "{} scored {} and would occupy New",
                row.candidate.tmdb_id.unwrap(),
                match_score(row)
            );
            assert!(!occupies_new(row));
        }
        assert!(
            occupies_new(&trek) || match_score(&trek) < MATCH_SCORE_FLOOR,
            "composer-linked Star Trek neighbors belong on New, got {}",
            match_score(&trek)
        );
        assert!(!occupies_explore(&trek));
    }

    #[test]
    fn mixed_filmography_and_unseeded_related_occupies_new() {
        let mut mixed = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.4,
            49_530,
        );
        mixed.eligibility.candidate_fit = 1.0;
        mixed.matched_features.push(neo_noir());
        mixed.candidate.sources.push(RetrievalSource {
            kind: RetrievalKind::Related,
            label: "similar to a catalog title".into(),
            seed_tmdb_id: None,
            seed_rating: None,
        });
        assert!(occupies_new(&mixed), "qualifying DP filmography must occupy New without a Related seed");
        assert!(!occupies_explore(&mixed));
    }

    #[test]
    fn several_loved_recommendation_seeds_can_backfill_filmography_new() {
        let mut mixed = row_with(vec![feat("composer", 2, 0.3)], false, false, 0.4, 700);
        mixed.candidate.sources = vec![
            RetrievalSource {
                kind: RetrievalKind::Filmography,
                label: "Composer Name".into(),
                seed_tmdb_id: None,
                seed_rating: None,
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "Loved One".into(),
                seed_tmdb_id: Some(1),
                seed_rating: Some(5.0),
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "Loved Two".into(),
                seed_tmdb_id: Some(2),
                seed_rating: Some(4.5),
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "Loved Three".into(),
                seed_tmdb_id: Some(3),
                seed_rating: Some(4.0),
            },
        ];
        mixed.eligibility.candidate_fit = 0.7;
        mixed.eligibility.evidence_grade = EvidenceGrade::Strong;
        assert!(mixed_recommendation_corroboration(&mixed));
        assert!(occupies_new(&mixed));

        mixed.candidate.sources.truncate(2);
        assert!(!mixed_recommendation_corroboration(&mixed));
        assert!(!occupies_new(&mixed));
    }

    #[test]
    fn composer_filmography_plus_related_stays_off_new() {
        let mut mixed = row_with(vec![feat("composer", 9, 0.55)], false, false, 0.4, 101);
        mixed.eligibility.candidate_fit = 1.0;
        mixed.eligibility.evidence_grade = EvidenceGrade::None;
        mixed.candidate.sources.push(RetrievalSource {
            kind: RetrievalKind::Related,
            label: "similar to Pulp Fiction".into(),
            seed_tmdb_id: Some(680),
            seed_rating: None,
        });
        assert!(!occupies_new(&mixed));
        assert!(match_score(&mixed) <= SINGLE_BRIDGE_CAP || occupies_explore(&mixed));
    }

    #[test]
    fn writer_filmography_is_not_padded_to_new() {
        let mut writer = row_with(vec![feat("writer", 3, 0.4)], false, false, 0.4, 88);
        writer.eligibility.candidate_fit = 1.0;
        assert!(
            match_score(&writer) < NEW_MATCH_FLOOR,
            "writer-only filmography must not use the 70 pad, got {}",
            match_score(&writer)
        );
        assert!(!occupies_new(&writer));
    }

    #[test]
    fn related_60_69_occupies_new() {
        let dogs = related_seed(vec![feat("director", 8, 0.5)], &["Crime", "Thriller"], 1.0, 500);
        assert!(
            match_score(&dogs) <= RELATED_ONLY_CAP,
            "Reservoir Dogs-class similar-to must not land at exact 70, got {}",
            match_score(&dogs)
        );
        if match_score(&dogs) >= MATCH_SCORE_FLOOR {
            assert!(occupies_new(&dogs));
            assert!(!occupies_explore(&dogs));
            assert_eq!(placement(&dogs), "new");
        }
    }

    #[test]
    fn yearless_stub_stays_off_both_boards() {
        let mut stub = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.4,
            1,
        );
        stub.candidate.year = None;
        stub.candidate.runtime = None;
        stub.candidate.vote_count = None;
        stub.eligibility.candidate_fit = 1.0;
        assert!(unreleased_display_row(&stub));
        assert!(!occupies_new(&stub));
        assert!(!occupies_explore(&stub));
        assert_eq!(filter_reason(&stub).as_deref(), Some("incomplete-metadata"));
    }

    #[test]
    fn yearless_with_runtime_can_occupy_new() {
        let mut related = related_seed(vec![feat("director", 4, 0.4)], &["Drama"], 1.0, 2);
        related.candidate.year = None;
        related.candidate.runtime = Some(110);
        related.candidate.vote_count = Some(200);
        assert!(!unreleased_display_row(&related));
        if match_score(&related) >= MATCH_SCORE_FLOOR {
            assert!(occupies_new(&related));
        }
        let mut filmography = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.4,
            3,
        );
        filmography.candidate.year = None;
        filmography.candidate.runtime = Some(110);
        filmography.candidate.vote_count = Some(200);
        filmography.eligibility.candidate_fit = 1.0;
        assert!(unreleased_new_row(&filmography));
        assert!(!occupies_new(&filmography));
        assert!(!occupies_explore(&filmography));
    }

    #[test]
    fn future_dated_stays_off_both_boards() {
        let mut future = row_with(
            vec![feat("cinematographer", 4, 0.45)],
            false,
            false,
            0.4,
            4,
        );
        future.candidate.year = Some(chrono::Utc::now().year() + 2);
        future.eligibility.candidate_fit = 1.0;
        assert!(unreleased_display_row(&future));
        assert!(!occupies_new(&future));
        assert!(!occupies_explore(&future));
        assert_eq!(filter_reason(&future).as_deref(), Some("unreleased"));
    }

    #[test]
    fn tv_movie_filmography_does_not_occupy_new() {
        let mut sketch = row_with(
            vec![feat("cinematographer", 3, 0.39)],
            false,
            false,
            0.4,
            49_001,
        );
        sketch.candidate.title = "Sketch Artist".into();
        sketch.candidate.genres = vec!["Crime".into(), "TV Movie".into()];
        sketch.eligibility.candidate_fit = 1.0;
        assert!(
            match_score(&sketch) <= SINGLE_BRIDGE_CAP,
            "TV movies must not be padded onto New, got {}",
            match_score(&sketch)
        );
        assert!(!occupies_new(&sketch));
        assert!(!occupies_explore(&sketch));
    }

    #[test]
    fn uncorroborated_dp_filmography_stays_off_new() {
        let mut blue = row_with(
            vec![feat("cinematographer", 4, 0.47)],
            false,
            false,
            0.4,
            13_922,
        );
        blue.candidate.title = "Out of the Blue".into();
        blue.eligibility.candidate_fit = 1.0;
        assert!(
            match_score(&blue) <= RELATED_ONLY_CAP,
            "a DP credit alone must not read as Very likely, got {}",
            match_score(&blue)
        );
        assert!(!occupies_new(&blue));
        assert!(!occupies_explore(&blue));
    }

    #[test]
    fn non_watchlist_never_says_very_likely() {
        let mut kts = row_with(
            vec![feat("cinematographer", 4, 0.47)],
            false,
            false,
            0.4,
            49_530,
        );
        kts.eligibility.candidate_fit = 1.0;
        kts.matched_features.push(neo_noir());
        assert!(occupies_new(&kts));
        assert!(
            match_score(&kts) <= NON_WATCHLIST_BAND_CAP,
            "New is Strong possibility, not Very likely, got {}",
            match_score(&kts)
        );
        let mut dune = row_with(
            vec![feat("cinematographer", 4, 0.47)],
            true,
            false,
            0.4,
            438_631,
        );
        dune.eligibility.candidate_fit = 1.0;
        dune.matched_features.push(neo_noir());
        assert!(
            match_score(&dune) > NON_WATCHLIST_BAND_CAP,
            "watchlist may stay Very likely, got {}",
            match_score(&dune)
        );
    }

    #[test]
    fn corroborated_recommendations_can_occupy_new() {
        let mut solo = row_with(
            vec![feat("writer", 3, 0.4), feat("composer", 9, 0.5)],
            false,
            true,
            0.8,
            348_350,
        );
        solo.candidate.sources = vec![
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from Rogue One: A Star Wars Story".into(),
                seed_tmdb_id: Some(330_459),
                seed_rating: None,
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from Avatar: Fire and Ash".into(),
                seed_tmdb_id: Some(835_33),
                seed_rating: None,
            },
        ];
        solo.eligibility.candidate_fit = 1.0;
        solo.candidate.title = "Solo: A Star Wars Story".into();
        assert!(
            !related_only(&solo),
            "corroborated recommendations must not be treated as a similar-to dump"
        );
        assert!(occupies_new(&solo), "neighbor of Rogue One with writer+composer should occupy New");
        assert!(!occupies_explore(&solo));
    }

    #[test]
    fn uncorroborated_recommendations_stay_related_only() {
        let mut insurgent = row_with(vec![feat("actor", 3, 0.4)], false, true, 0.8, 262_504);
        insurgent.candidate.sources = vec![RetrievalSource {
            kind: RetrievalKind::RelatedRecommendations,
            label: "recommended from The Hunger Games: Catching Fire".into(),
            seed_tmdb_id: Some(101_299),
            seed_rating: None,
        }];
        insurgent.candidate.title = "Insurgent".into();
        insurgent.eligibility.evidence_grade = EvidenceGrade::None;
        assert!(related_only(&insurgent));
        assert!(!occupies_new(&insurgent));
    }

    #[test]
    fn similar_only_stays_capped_but_can_occupy_new() {
        let mut similar = row_with(vec![feat("director", 6, 0.5)], false, true, 0.8, 116);
        similar.matched_features.push(neo_noir());
        similar.candidate.sources = vec![RetrievalSource {
            kind: RetrievalKind::RelatedSimilar,
            label: "similar to Last Night in Soho".into(),
            seed_tmdb_id: Some(565_123),
            seed_rating: None,
        }];
        assert!(related_only(&similar));
        assert!(match_score(&similar) <= RELATED_ONLY_CAP);
        if match_score(&similar) >= MATCH_SCORE_FLOOR {
            assert!(occupies_new(&similar));
        }
    }

    #[test]
    fn related_only_family_does_not_occupy_explore() {
        let mut kids = related_seed(
            vec![feat("director", 4, 0.4)],
            &["Family", "Comedy"],
            1.0,
            351_837,
        );
        kids.eligibility.evidence_grade = EvidenceGrade::None;
        assert!(!occupies_new(&kids));
        assert!(!occupies_explore(&kids));
    }

    #[test]
    fn multi_seed_recommendations_occupy_new() {
        let mut trek = row_with(vec![feat("composer", 4, 0.24)], false, true, 0.5, 13_475);
        trek.candidate.title = "Star Trek".into();
        trek.candidate.genres = vec!["Science Fiction".into(), "Action".into()];
        trek.candidate.sources = vec![
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from Rogue One: A Star Wars Story".into(),
                seed_tmdb_id: Some(330_459),
                seed_rating: None,
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from Avatar".into(),
                seed_tmdb_id: Some(19_995),
                seed_rating: None,
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from Interstellar".into(),
                seed_tmdb_id: Some(157_336),
                seed_rating: None,
            },
            RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from The Mandalorian and Grogu".into(),
                seed_tmdb_id: Some(1_228_710),
                seed_rating: None,
            },
        ];
        assert!(
            match_score(&trek) >= MATCH_SCORE_FLOOR,
            "Rogue One/Avatar/Interstellar recs must not sit at 5%, got {}",
            match_score(&trek)
        );
        assert!(occupies_new(&trek));
        assert!(!occupies_explore(&trek));
        assert!(match_score(&trek) <= RELATED_ONLY_CAP);
    }

    #[test]
    fn single_seed_recommendation_does_not_invent_a_match() {
        let mut curves = related_seed(
            vec![feat("actor", 3, 0.12)],
            &["Drama", "Comedy"],
            1.0,
            10_337,
        );
        curves.candidate.sources = vec![RetrievalSource {
            kind: RetrievalKind::RelatedRecommendations,
            label: "recommended from Tony".into(),
            seed_tmdb_id: Some(1_329_016),
            seed_rating: None,
        }];
        curves.candidate.title = "Real Women Have Curves".into();
        curves.eligibility.evidence_grade = EvidenceGrade::None;
        assert!(
            match_score(&curves) < MATCH_SCORE_FLOOR,
            "one weak seed must not mint a New card, got {}",
            match_score(&curves)
        );
        assert!(!occupies_new(&curves));
    }
}
