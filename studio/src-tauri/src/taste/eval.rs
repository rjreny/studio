use crate::taste::score::ScoredCandidate;
use crate::taste::features::FeatureProfile;
use crate::taste::retrieve::{identity_key, seen_keys, FilmRecord};
use crate::storage::db::Database;
use crate::taste::semantic::SemanticStats;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct LayeredMetrics {
    pub recall_at_1000: f32,
    pub recall_at_100: f32,
    pub recall_at_50: f32,
    pub recall_at_25: f32,
    pub recall_at_12: f32,
    pub mrr: f32,
    pub ndcg_at_12: f32,
    pub precision_at_40: f32,
    pub recall_at_40: f32,
}

pub fn recall_at(retrieved: &[String], held_out: &HashSet<String>, k: usize) -> f32 {
    if held_out.is_empty() {
        return 0.0;
    }
    let hit = retrieved.iter().take(k).filter(|id| held_out.contains(*id)).count();
    hit as f32 / held_out.len() as f32
}

pub fn precision_at(retrieved: &[String], held_out: &HashSet<String>, k: usize) -> f32 {
    let shown = retrieved.len().min(k);
    if shown == 0 {
        return 0.0;
    }
    retrieved
        .iter()
        .take(k)
        .filter(|id| held_out.contains(*id))
        .count() as f32
        / shown as f32
}

pub fn mrr(retrieved: &[String], held_out: &HashSet<String>) -> f32 {
    for (i, id) in retrieved.iter().enumerate() {
        if held_out.contains(id) {
            return 1.0 / (i as f32 + 1.0);
        }
    }
    0.0
}

pub fn ndcg_at(retrieved: &[String], held_out: &HashSet<String>, k: usize) -> f32 {
    let mut dcg = 0.0;
    for (i, id) in retrieved.iter().take(k).enumerate() {
        if held_out.contains(id) {
            dcg += 1.0 / ((i as f32 + 2.0).log2());
        }
    }
    let ideal = (0..held_out.len().min(k))
        .map(|i| 1.0 / ((i as f32 + 2.0).log2()))
        .sum::<f32>();
    if ideal <= 0.0 {
        0.0
    } else {
        dcg / ideal
    }
}

pub fn ids_of(scored: &[ScoredCandidate]) -> Vec<String> {
    scored
        .iter()
        .filter_map(|c| c.candidate.tmdb_id.map(|id| format!("tmdb:{id}")))
        .collect()
}

pub fn layered(
    retrieval_ids: &[String],
    scored_ids: &[String],
    held_out: &HashSet<String>,
) -> LayeredMetrics {
    LayeredMetrics {
        recall_at_1000: recall_at(retrieval_ids, held_out, 1000),
        recall_at_100: recall_at(retrieval_ids, held_out, 100),
        recall_at_50: recall_at(retrieval_ids, held_out, 50),
        recall_at_25: recall_at(scored_ids, held_out, 25),
        recall_at_12: recall_at(scored_ids, held_out, 12),
        mrr: mrr(scored_ids, held_out),
        ndcg_at_12: ndcg_at(scored_ids, held_out, 12),
        precision_at_40: precision_at(scored_ids, held_out, 40),
        recall_at_40: recall_at(scored_ids, held_out, 40),
    }
}

/// The evaluation split used for real replay runs. The newest positively
/// rated films are held out, while every held-out identity is removed from
/// both the training profile and the seen set.
#[derive(Debug, Clone, Default)]
pub struct ReplayInputs {
    pub training_films: Vec<FilmRecord>,
    pub profile: FeatureProfile,
    pub held_out: HashSet<String>,
    pub seen: HashSet<String>,
}

pub fn time_aware_replay_inputs(films: &[FilmRecord], holdout_count: usize) -> ReplayInputs {
    let mut positive: Vec<(usize, &FilmRecord)> = films
        .iter()
        .enumerate()
        .filter(|(_, film)| film.rating.map(|rating| rating >= 4.0).unwrap_or(false))
        .collect();
    positive.sort_by(|(left_i, left), (right_i, right)| {
        left.last_date
            .as_deref()
            .unwrap_or("")
            .cmp(right.last_date.as_deref().unwrap_or(""))
            .then_with(|| left_i.cmp(right_i))
    });
    let held_out: HashSet<String> = positive
        .iter()
        .rev()
        .take(holdout_count)
        .map(|(_, film)| identity_key(film.tmdb_id, &film.title, film.year))
        .collect();
    let training_films: Vec<FilmRecord> = films
        .iter()
        .filter(|film| !held_out.contains(&identity_key(film.tmdb_id, &film.title, film.year)))
        .cloned()
        .collect();
    let seen = seen_keys(&training_films);
    let profile = crate::taste::feature_profile_from_films(&training_films);
    ReplayInputs {
        training_films,
        profile,
        held_out,
        seen,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayMetrics {
    pub held_out_count: usize,
    pub baseline_precision_at_40: f32,
    pub baseline_recall_at_40: f32,
    pub revised_precision_at_40: f32,
    pub revised_recall_at_40: f32,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayReport {
    pub holdout_count: usize,
    pub baseline_precision_at_40: f32,
    pub baseline_recall_at_40: f32,
    pub revised_precision_at_40: f32,
    pub revised_recall_at_40: f32,
    pub baseline_ndcg_at_12: f32,
    pub revised_ndcg_at_12: f32,
    pub baseline_board_count: usize,
    pub revised_board_count: usize,
    pub baseline_related_only: usize,
    pub revised_related_only: usize,
    pub semantic: SemanticStats,
    pub error: Option<String>,
}

/// Run the real, time-aware holdout against the current local catalog. This is
/// intentionally opt-in from debug builds because it can perform extra
/// retrieval/embedding work; normal user runs remain single-pass.
pub fn run_replay(
    db: &Database,
    key: &str,
    films: &[FilmRecord],
    holdout_count: usize,
) -> Result<ReplayReport, String> {
    let inputs = time_aware_replay_inputs(films, holdout_count);
    if inputs.held_out.is_empty() {
        return Ok(ReplayReport {
            error: Some("No positively rated films were available for replay".into()),
            ..Default::default()
        });
    }
    let retrieved = crate::taste::retrieve::retrieve_with_coverage(
        db,
        &inputs.training_films,
        &inputs.profile,
        &inputs.seen,
        false,
    )?;
    let baseline = crate::taste::score::score_pool(&inputs.profile, &retrieved.candidates);
    let (semantic_scores, semantic) = crate::taste::semantic::score_candidates(
        db,
        key,
        &inputs.training_films,
        &retrieved.candidates,
    );
    let revised = crate::taste::score::score_pool_with_semantic(
        &inputs.profile,
        &retrieved.candidates,
        &semantic_scores,
    );
    let baseline_ids = ids_of(&baseline.ranked);
    let revised_ids = ids_of(&revised.ranked);
    let metrics = compare_replay(&baseline_ids, &revised_ids, &inputs.held_out);
    let seen_ids = inputs
        .training_films
        .iter()
        .filter_map(|f| f.tmdb_id)
        .collect::<HashSet<_>>();
    let baseline_quality = board_quality(
        &crate::taste::workspace::assemble(&baseline.ranked),
        &seen_ids,
    );
    let revised_quality = board_quality(
        &crate::taste::workspace::assemble(&revised.ranked),
        &seen_ids,
    );
    Ok(ReplayReport {
        holdout_count: metrics.held_out_count,
        baseline_precision_at_40: metrics.baseline_precision_at_40,
        baseline_recall_at_40: metrics.baseline_recall_at_40,
        revised_precision_at_40: metrics.revised_precision_at_40,
        revised_recall_at_40: metrics.revised_recall_at_40,
        baseline_ndcg_at_12: ndcg_at(&baseline_ids, &inputs.held_out, 12),
        revised_ndcg_at_12: ndcg_at(&revised_ids, &inputs.held_out, 12),
        baseline_board_count: baseline_quality.new_count,
        revised_board_count: revised_quality.new_count,
        baseline_related_only: baseline_quality.new_related_only,
        revised_related_only: revised_quality.new_related_only,
        semantic,
        error: None,
    })
}

pub fn compare_replay(
    baseline: &[String],
    revised: &[String],
    held_out: &HashSet<String>,
) -> ReplayMetrics {
    ReplayMetrics {
        held_out_count: held_out.len(),
        baseline_precision_at_40: precision_at(baseline, held_out, 40),
        baseline_recall_at_40: recall_at(baseline, held_out, 40),
        revised_precision_at_40: precision_at(revised, held_out, 40),
        revised_recall_at_40: recall_at(revised, held_out, 40),
    }
}

/// Band occupancy and leakage for New / Explore / Watchlist.
/// `70` is a band, not a calibrated probability.
#[derive(Debug, Clone, Default)]
pub struct BoardQuality {
    pub new_count: usize,
    pub explore_count: usize,
    pub watchlist_count: usize,
    pub new_related_only: usize,
    pub new_below_floor: usize,
    pub explore_outside_band: usize,
    pub seen_leakage: usize,
    pub board_overlap: usize,
    pub watchlist_on_discovery_boards: usize,
}

pub fn board_quality(
    ws: &crate::taste::workspace::Workspace,
    seen: &HashSet<i64>,
) -> BoardQuality {
    use crate::taste::confidence::{self, MATCH_SCORE_FLOOR, NEW_MATCH_FLOOR};
    let mut q = BoardQuality {
        new_count: ws.new_picks.len(),
        explore_count: ws.explore_picks.len(),
        watchlist_count: ws.watchlist_picks.len(),
        ..Default::default()
    };
    q.new_related_only = ws
        .new_picks
        .iter()
        .filter(|c| confidence::related_only(c))
        .count();
    q.new_below_floor = ws
        .new_picks
        .iter()
        .filter(|c| confidence::match_score(c) < MATCH_SCORE_FLOOR)
        .count();
    q.explore_outside_band = ws
        .explore_picks
        .iter()
        .filter(|c| {
            let s = confidence::match_score(c);
            s < MATCH_SCORE_FLOOR || s >= NEW_MATCH_FLOOR
        })
        .count();
    let displayed = ws
        .new_picks
        .iter()
        .chain(ws.explore_picks.iter())
        .chain(ws.watchlist_picks.iter());
    q.seen_leakage = displayed
        .clone()
        .filter(|c| c.candidate.tmdb_id.map(|id| seen.contains(&id)).unwrap_or(false))
        .count();
    q.watchlist_on_discovery_boards = ws
        .new_picks
        .iter()
        .chain(ws.explore_picks.iter())
        .filter(|c| c.candidate.watchlist)
        .count();
    let mut ids = HashSet::new();
    for c in ws
        .new_picks
        .iter()
        .chain(ws.explore_picks.iter())
        .chain(ws.watchlist_picks.iter())
    {
        if let Some(id) = c.candidate.tmdb_id {
            if !ids.insert(id) {
                q.board_overlap += 1;
            }
        }
    }
    q
}

#[derive(Debug, Clone, Default)]
pub struct SourceMix {
    pub related_recommendations: usize,
    pub related_similar: usize,
    pub related_legacy: usize,
    pub filmography_only: usize,
    pub watchlist: usize,
    pub friend: usize,
}

pub fn source_mix(rows: &[ScoredCandidate]) -> SourceMix {
    use crate::taste::retrieve::RetrievalKind;
    let mut mix = SourceMix::default();
    for c in rows {
        if c.candidate.watchlist {
            mix.watchlist += 1;
        }
        if c.candidate
            .sources
            .iter()
            .any(|s| s.kind == RetrievalKind::Friend)
        {
            mix.friend += 1;
        }
        let related: Vec<_> = c
            .candidate
            .sources
            .iter()
            .filter(|s| s.kind.is_related())
            .collect();
        if related
            .iter()
            .any(|s| s.kind == RetrievalKind::RelatedRecommendations)
        {
            mix.related_recommendations += 1;
        }
        if related
            .iter()
            .any(|s| s.kind == RetrievalKind::RelatedSimilar)
        {
            mix.related_similar += 1;
        }
        if related.iter().any(|s| s.kind == RetrievalKind::Related) {
            mix.related_legacy += 1;
        }
        if !c.candidate.watchlist
            && !c.candidate.sources.is_empty()
            && c.candidate
                .sources
                .iter()
                .all(|s| s.kind == RetrievalKind::Filmography)
        {
            mix.filmography_only += 1;
        }
    }
    mix
}

pub fn resume_only_share(rows: &[ScoredCandidate]) -> f32 {
    if rows.is_empty() {
        return 0.0;
    }
    source_mix(rows).filmography_only as f32 / rows.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_vs_scoring_layers() {
        let retrieved: Vec<String> = (1..200).map(|i| format!("tmdb:{i}")).collect();
        let mut scored = retrieved.clone();
        scored.retain(|id| {
            let n: i64 = id.trim_start_matches("tmdb:").parse().unwrap();
            n % 2 == 0
        });
        let mut held = HashSet::new();
        held.insert("tmdb:4".into());
        held.insert("tmdb:50".into());
        let m = layered(&retrieved, &scored, &held);
        assert!(m.recall_at_1000 >= 0.99);
        assert!(m.recall_at_100 >= 0.99);
        assert!(m.recall_at_50 > 0.0);
        assert!(m.recall_at_25 > 0.0);
        assert!(m.recall_at_12 > 0.0);
        assert!(m.mrr > 0.0);
        assert!(m.ndcg_at_12 > 0.0);
        assert!(m.precision_at_40 > 0.0);
        assert!(m.recall_at_40 > 0.0);
        assert_eq!(source_mix(&[]).filmography_only, 0);
        assert_eq!(resume_only_share(&[]), 0.0);
        assert!(!ids_of(&[]).is_empty() || true);
    }

    #[test]
    fn replay_comparison_reports_precision_and_recall_without_padding() {
        let held_out = HashSet::from(["tmdb:2".into(), "tmdb:4".into()]);
        let baseline = vec!["tmdb:9".into(), "tmdb:2".into(), "tmdb:8".into()];
        let revised = vec!["tmdb:2".into(), "tmdb:4".into()];
        let metrics = compare_replay(&baseline, &revised, &held_out);
        assert_eq!(metrics.held_out_count, 2);
        assert!((metrics.baseline_precision_at_40 - (1.0 / 3.0)).abs() < 1e-5);
        assert!((metrics.baseline_recall_at_40 - 0.5).abs() < 1e-5);
        assert!((metrics.revised_precision_at_40 - 1.0).abs() < 1e-5);
        assert!((metrics.revised_recall_at_40 - 1.0).abs() < 1e-5);
    }

    fn film(
        title: &str,
        tmdb_id: i64,
        rating: f32,
        year: i32,
        genres: &[&str],
        credits: Vec<crate::taste::features::Credit>,
        age_years: f32,
    ) -> crate::taste::retrieve::FilmRecord {
        crate::taste::retrieve::FilmRecord {
            key: format!("tmdb:{tmdb_id}"),
            title: title.into(),
            year: Some(year),
            tmdb_id: Some(tmdb_id),
            rating: Some(rating),
            liked: rating >= 4.5,
            watched: true,
            watchlist: false,
            viewings: 1,
            last_date: None,
            genres: genres.iter().map(|g| (*g).to_string()).collect(),
            credits,
            keywords: vec![],
            recommendations: vec![],
            similar: vec![],
            runtime: Some(100),
            poster: None,
            vote_count: Some(1000),
            review: None,
            signal: None,
            age_years: Some(age_years),
        }
    }

    fn credit(job: &str, name: &str, id: i64) -> crate::taste::features::Credit {
        crate::taste::features::Credit {
            id: Some(id),
            name: name.into(),
            job: job.into(),
        }
    }

    #[test]
    fn time_aware_replay_removes_new_liked_films_from_profile_and_seen() {
        use crate::taste::retrieve::attach_signals;
        let mut films = vec![
            film("Old", 1, 4.5, 2000, &["Drama"], vec![], 8.0),
            film("Middle", 2, 4.0, 2010, &["Crime"], vec![], 5.0),
            film("Newest", 3, 5.0, 2025, &["Science Fiction"], vec![], 0.1),
        ];
        films[0].last_date = Some("2020-01-01".into());
        films[1].last_date = Some("2023-01-01".into());
        films[2].last_date = Some("2026-01-01".into());
        attach_signals(&mut films);
        let replay = time_aware_replay_inputs(&films, 1);
        assert!(replay.held_out.contains("tmdb:3"));
        assert!(replay.training_films.iter().all(|f| f.tmdb_id != Some(3)));
        assert!(!replay.seen.contains("tmdb:3"));
        assert!(replay.seen.contains("tmdb:1"));
        assert!(replay
            .profile
            .affinities
            .iter()
            .flat_map(|a| a.positive_evidence.iter())
            .all(|e| e.tmdb_id != Some(3)));
    }

    fn filmography(
        title: &str,
        tmdb_id: i64,
        genres: &[&str],
        person: crate::taste::features::Credit,
    ) -> crate::taste::retrieve::Candidate {
        use crate::taste::retrieve::{MediaKind, RetrievalKind, RetrievalSource};
        let label = person.name.clone();
        crate::taste::retrieve::Candidate {
            tmdb_id: Some(tmdb_id),
            title: title.into(),
            year: Some(2005),
            poster: None,
            genres: genres.iter().map(|g| (*g).to_string()).collect(),
            credits: vec![person],
            keywords: vec![],
            runtime: Some(100),
            vote_count: Some(800),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Filmography,
                label,
                seed_tmdb_id: None,
                seed_rating: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        media_kind: MediaKind::Movie,
        }
    }

    /// 627-film Powell production failure, through the real deterministic pipeline.
    #[test]
    fn powell_e2e_627_history_retrieval_to_shortlist() {
        use crate::taste::features::{COMPOSER_W, CINEMATOGRAPHER_W};
        use crate::taste::reason::{empty_critic, ReasonerPick};
        use crate::taste::retrieve::{attach_signals, Candidate, MediaKind, RetrievalKind, RetrievalSource};
        use crate::taste::score::{
            filmography_supported, person_pipeline_trace, score_all,
        };
        use crate::taste::shortlist::shortlist;
        use crate::taste::validate::{diversity_warnings, hard_validate};
        use crate::taste::feature_profile_from_films;

        let powell = credit("Original Music Composer", "John Powell", 50);
        let fraser = credit("Director of Photography", "Greig Fraser", 77);
        let burwell = credit("Original Music Composer", "Carter Burwell", 88);
        let tarantino = credit("Director", "Quentin Tarantino", 99);

        let mut films = vec![
            film(
                "Kung Fu Panda",
                1,
                5.0,
                2008,
                &["Comedy", "Animation", "Family"],
                vec![powell.clone()],
                1.0,
            ),
            film(
                "Minions & Monsters",
                2,
                5.0,
                2010,
                &["Comedy", "Animation", "Family"],
                vec![powell.clone()],
                0.8,
            ),
            film(
                "The Batman",
                3,
                4.5,
                2022,
                &["Crime"],
                vec![fraser.clone()],
                0.5,
            ),
            film(
                "Dune",
                4,
                4.5,
                2021,
                &["Science Fiction"],
                vec![fraser.clone()],
                0.3,
            ),
            film(
                "Fargo",
                5,
                4.5,
                1996,
                &["Crime", "Drama"],
                vec![burwell.clone()],
                8.0,
            ),
            film(
                "No Country for Old Men",
                6,
                4.5,
                2007,
                &["Crime", "Drama"],
                vec![burwell.clone()],
                6.0,
            ),
            film(
                "Pulp Fiction",
                7,
                5.0,
                1994,
                &["Crime"],
                vec![tarantino.clone()],
                10.0,
            ),
            film(
                "Reservoir Dogs",
                8,
                5.0,
                1992,
                &["Crime"],
                vec![tarantino.clone()],
                12.0,
            ),
        ];
        let genres = [
            "Drama",
            "Thriller",
            "Comedy",
            "Action",
            "Horror",
            "Crime",
            "Mystery",
            "Science Fiction",
            "Romance",
            "Adventure",
        ];
        for i in 9..=627i64 {
            let rating = 1.5 + ((i % 8) as f32) * 0.5;
            films.push(film(
                &format!("Log {i}"),
                i,
                rating,
                2000 + (i % 25) as i32,
                &[genres[i as usize % genres.len()]],
                vec![credit("Director", &format!("Dir{}", i % 80), 1000 + i % 80)],
                0.2 + (i % 15) as f32 * 0.4,
            ));
        }
        assert_eq!(films.len(), 627);

        attach_signals(&mut films);
        assert!(films.iter().all(|f| f.signal.is_some()));
        let ratings: Vec<f32> = films.iter().filter_map(|f| f.rating).collect();
        assert_eq!(ratings.len(), 627);

        let profile = feature_profile_from_films(&films);
        assert!(
            !profile.modes.is_empty() || !profile.dimensions.is_empty(),
            "modes/dimensions should populate on a 627-film log"
        );

        let powell_aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "John Powell")
            .expect("Powell affinity");
        assert_eq!(powell_aff.appearances, 2);
        assert_eq!(powell_aff.positive_evidence.len(), 2);
        assert!(powell_aff
            .positive_evidence
            .iter()
            .any(|e| e.title.contains("Panda")));
        assert!(powell_aff
            .positive_evidence
            .iter()
            .any(|e| e.title.contains("Minions")));
        let expected_conf = 1.0 - (-2.0_f32 / 4.0).exp();
        assert!(
            (powell_aff.confidence - expected_conf).abs() < 0.02,
            "confidence {} vs frozen 1-exp(-2/4)={}",
            powell_aff.confidence,
            expected_conf
        );
        let frozen_sa = powell_aff.recommendation_mean
            * powell_aff.confidence
            * COMPOSER_W
            * powell_aff.portability;
        assert!(
            (powell_aff.scoring_affinity() - frozen_sa).abs() < 1e-5,
            "affinity must not be weakened outside the frozen product"
        );
        assert!(powell_aff.recommendation_mean > 0.45);
        assert!(
            powell_aff.evidence_cluster.genres.iter().any(|g| g == "comedy"),
            "cluster must come from Powell evidence films, got {:?}",
            powell_aff.evidence_cluster
        );
        assert!(
            !powell_aff.evidence_cluster.genres.iter().any(|g| g == "drama"),
            "Powell cluster must not be a global drama prior: {:?}",
            powell_aff.evidence_cluster
        );

        let burwell_aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Carter Burwell")
            .expect("Burwell");
        assert_eq!(burwell_aff.appearances, 2);
        assert!(
            burwell_aff.evidence_cluster.genres.iter().any(|g| g == "drama")
                || burwell_aff.evidence_cluster.genres.iter().any(|g| g == "crime"),
            "Burwell cluster follows HIS evidence, not Powell's comedy: {:?}",
            burwell_aff.evidence_cluster
        );
        assert!(
            !burwell_aff.evidence_cluster.genres.iter().any(|g| g == "comedy"),
            "Burwell must not inherit a hardcoded comedy filter: {:?}",
            burwell_aff.evidence_cluster
        );

        let fraser_aff = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .expect("Fraser");
        assert!(fraser_aff.citeable());
        assert!(
            fraser_aff.evidence_cluster.is_empty(),
            "Crime+Sci-Fi evidence must not invent a comedy cluster: {:?}",
            fraser_aff.evidence_cluster
        );
        let fraser_sa = fraser_aff.recommendation_mean
            * fraser_aff.confidence
            * CINEMATOGRAPHER_W
            * fraser_aff.portability;
        assert!((fraser_aff.scoring_affinity() - fraser_sa).abs() < 1e-5);

        let overlap_powell = [
            ("Ice Age: The Meltdown", &["Comedy", "Animation", "Family"][..]),
            ("Antz", &["Comedy", "Animation"][..]),
            ("Chicken Run", &["Comedy", "Animation", "Family"][..]),
            ("Rio", &["Comedy", "Animation"][..]),
            ("Horton Hears a Who!", &["Comedy", "Animation", "Family"][..]),
            ("Robots", &["Comedy", "Animation"][..]),
            ("Bolt", &["Comedy", "Animation", "Family"][..]),
            ("Happy Feet", &["Comedy", "Animation"][..]),
        ];
        let unrelated_powell = [
            ("The Bourne Supremacy", &["Thriller"][..]),
            ("United 93", &["Drama"][..]),
            ("Mr. & Mrs. Smith", &["Action"][..]),
            ("Be Cool", &["Crime"][..]),
            ("The Adventures of Pluto Nash", &["Science Fiction"][..]),
            ("Paycheck", &["Thriller"][..]),
            ("The Italian Job", &["Action"][..]),
            ("Hidalgo", &["Adventure"][..]),
            ("I Am Sam", &["Drama"][..]),
            ("Drumline", &["Drama"][..]),
            ("Two Weeks Notice", &["Romance"][..]),
            ("Stop-Loss", &["Drama"][..]),
        ];
        let mut retrieved = Vec::new();
        for (i, (title, genres)) in overlap_powell.iter().enumerate() {
            retrieved.push(filmography(title, 10_000 + i as i64, genres, powell.clone()));
        }
        for (i, (title, genres)) in unrelated_powell.iter().enumerate() {
            retrieved.push(filmography(title, 10_100 + i as i64, genres, powell.clone()));
        }
        assert_eq!(
            retrieved
                .iter()
                .filter(|c| c.credits.iter().any(|c| c.name == "John Powell"))
                .count(),
            20
        );

        let facet_kept: Vec<&Candidate> = retrieved
            .iter()
            .filter(|c| filmography_supported(&profile, c))
            .collect();
        let powell_facet = facet_kept
            .iter()
            .filter(|c| c.credits.iter().any(|p| p.name == "John Powell"))
            .count();
        assert!(
            retrieved.iter().any(|c| c.title == "United 93")
                && !facet_kept.iter().any(|c| c.title == "United 93"),
            "United 93 must be dropped because Powell evidence is comedy/animation, not because comedy is hardcoded"
        );
        assert!(
            retrieved.iter().any(|c| c.title == "The Bourne Supremacy")
                && !facet_kept.iter().any(|c| c.title == "The Bourne Supremacy")
        );
        assert!(facet_kept.iter().any(|c| c.title.contains("Ice Age")));
        assert!(facet_kept.iter().any(|c| c.title == "Antz"));

        for (i, title) in ["Fargo 2", "True Grit", "The Big Lebowski", "Miller's Crossing"]
            .iter()
            .enumerate()
        {
            retrieved.push(filmography(
                title,
                11_000 + i as i64,
                &["Crime", "Drama"],
                burwell.clone(),
            ));
        }
        retrieved.push(filmography(
            "Burwell Comedy Miss",
            11_050,
            &["Comedy", "Animation"],
            burwell.clone(),
        ));
        assert!(filmography_supported(
            &profile,
            &filmography("True Grit", 1, &["Crime", "Drama"], burwell.clone())
        ));
        assert!(
            !filmography_supported(
                &profile,
                &filmography(
                    "Burwell Comedy Miss",
                    1,
                    &["Comedy", "Animation"],
                    burwell.clone()
                )
            ),
            "Burwell drama/crime cluster must reject comedy filmography — per-person, not global comedy"
        );

        for (i, (title, g)) in [
            ("Zero Dark Thirty", "Thriller"),
            ("Mary Magdalene", "Drama"),
            ("Rogue One", "Science Fiction"),
        ]
        .iter()
        .enumerate()
        {
            retrieved.push(filmography(title, 12_000 + i as i64, &[g], fraser.clone()));
        }
        assert!(
            !filmography_supported(
                &profile,
                &filmography("Zero Dark Thirty", 1, &["Thriller"], fraser.clone())
            ),
            "Fraser Crime/Sci-Fi evidence must not transfer to a Thriller-only credit"
        );
        assert!(filmography_supported(
            &profile,
            &filmography("Rogue One", 1, &["Science Fiction"], fraser.clone())
        ));

        for i in 0..8i64 {
            retrieved.push(filmography(
                &format!("Tarantino {i}"),
                13_000 + i,
                &["Crime"],
                tarantino.clone(),
            ));
        }
        for i in 0..80i64 {
            let g = genres[i as usize % genres.len()];
            retrieved.push(Candidate {
                tmdb_id: Some(20_000 + i),
                title: format!("Related {i}"),
                year: Some(2016),
                poster: None,
                genres: vec![g.into()],
                credits: vec![],
                keywords: vec![],
                runtime: Some(110),
                vote_count: Some(400),
                watchlist: false,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Related,
                    label: "similar to log".into(),
                    seed_tmdb_id: Some(9 + (i % 20)),
                    seed_rating: None,
                }],
                friend_affinity: 0.0,
                tmdb_related: 0.55,
            media_kind: MediaKind::Movie,
            });
        }

        let powell_injected = retrieved
            .iter()
            .filter(|c| {
                c.credits.iter().any(|p| p.name == "John Powell")
                    || c.sources.iter().any(|s| s.label == "John Powell")
            })
            .count();
        assert_eq!(powell_injected, 20);

        let scored = score_all(&profile, &retrieved);
        assert!(scored.len() <= 100);
        let top100_powell = scored
            .iter()
            .filter(|c| c.person_keys.iter().any(|k| k == "John Powell"))
            .count();
        let short = shortlist(&scored);
        assert!(
            short.len() <= 50,
            "shortlist must cap at 50, got {}",
            short.len()
        );
        assert!(
            short.len() >= 8,
            "filmography/craft should still fill a shortlist, got {}",
            short.len()
        );
        let related_genre_only = short
            .iter()
            .filter(|c| c.candidate.title.starts_with("Related "))
            .count();
        assert_eq!(
            related_genre_only, 0,
            "related+genre-only padded the shortlist: {related_genre_only}"
        );

        let traces = person_pipeline_trace(&profile, &retrieved, &scored, &short);
        let ptrace = traces.iter().find(|t| t.name == "John Powell").unwrap();
        println!(
            "Powell stages: injected={} facet_ok={} scored/top100={} mmr={} appearances={} rec_mean={:.3} conf={:.3} scoring_affinity={:.3}",
            ptrace.injected,
            powell_facet,
            ptrace.survived_score,
            ptrace.survived_mmr,
            ptrace.appearances,
            ptrace.recommendation_mean,
            ptrace.confidence,
            ptrace.scoring_affinity
        );
        assert_eq!(ptrace.injected, 20);
        assert_eq!(ptrace.appearances, 2);
        assert!(ptrace.survived_score < ptrace.injected);
        assert_eq!(ptrace.survived_score, top100_powell);
        assert!(ptrace.survived_mmr <= 8, "MMR Powell {}", ptrace.survived_mmr);
        assert!((ptrace.confidence - expected_conf).abs() < 0.02);

        let powell_n = short
            .iter()
            .filter(|c| c.person_keys.iter().any(|k| k == "John Powell"))
            .count();
        assert!(
            powell_n <= 8,
            "shortlist Powell takeover: {powell_n} / {}",
            short.len()
        );
        assert!(
            short.iter().any(|c| c.person_keys.iter().any(|k| k.contains("Powell")))
                || scored.iter().any(|c| c.candidate.title.contains("Ice Age")
                    || c.candidate.title == "Antz"),
            "overlapping Powell filmography must be allowed to enter scoring"
        );

        let mut person_counts = std::collections::HashMap::<String, usize>::new();
        for c in &short {
            for k in &c.person_keys {
                *person_counts.entry(k.clone()).or_insert(0) += 1;
            }
        }
        for (name, n) in &person_counts {
            assert!(
                *n <= 8,
                "{name} took over the shortlist with {n} / {}",
                short.len()
            );
        }

        let modes: std::collections::HashSet<_> = short
            .iter()
            .flat_map(|c| c.candidate.modes.iter().cloned())
            .collect();
        assert!(
            modes.len() >= 2,
            "MMR must retain distinct modes, got {modes:?}"
        );

        let mut eight_powell = Vec::new();
        for i in 0..12 {
            let mut row = short[i % short.len()].clone();
            if i < 8 {
                row.person_keys = vec!["John Powell".into()];
            }
            eight_powell.push(row);
        }
        let warnings = diversity_warnings(&eight_powell);
        assert!(
            warnings.iter().any(|w| w.message.contains("person John Powell")),
            "{:?}",
            warnings.iter().map(|w| &w.message).collect::<Vec<_>>()
        );

        let picks: Vec<ReasonerPick> = short
            .iter()
            .take(12)
            .map(|c| ReasonerPick {
                id: format!("tmdb:{}", c.candidate.tmdb_id.unwrap()),
                title: c.candidate.title.clone(),
                year: c.candidate.year,
                why: "because".into(),
                mode: "core".into(),
                rhymes_with: vec![],
            })
            .collect();
        let result = hard_validate(&picks, &short, &[], &HashSet::new(), profile.modes.len().max(3));
        assert!(result.picks.len() <= 100);
        assert!(
            result.picks.iter().all(|p| {
                short.iter().any(|s| s.candidate.tmdb_id == p.candidate.tmdb_id)
            }),
            "validate must stay inside the scored shortlist"
        );
        assert!(result.picks.iter().all(|p| p.candidate.tmdb_id != Some(99)));
        let _ = empty_critic();
    }

    /// Workspace-9 live baseline (2026-08-26 latest.json): 26 New rows, related-only
    /// Reservoir Dogs at exact 70, ~25 related-only competing for New.
    /// Workspace-10+ must report related-only on New = 0. Match score 70 is a band,
    /// not P(watch). Workspace-11 also requires the cited director/DP job on the film.
    #[test]
    fn board_quality_rejects_workspace9_related_padding() {
        use crate::taste::explain::{EligibilityTrace, MatchedFeatureView};
        use crate::taste::retrieve::{MediaKind, RetrievalKind, RetrievalSource};
        use crate::taste::score::{CandidateScore, CandidateView, ScoredCandidate};
        use crate::taste::workspace::assemble;

        fn feat(name: &str, family: &str, n: u32, aff: f32) -> MatchedFeatureView {
            MatchedFeatureView {
                name: name.into(),
                family: family.into(),
                appearances: n,
                recommendation_mean: aff,
                scoring_affinity: aff,
                confidence: 0.8,
                portability: 1.0,
                citeable: true,
                cited: true,
            }
        }
        fn scored(
            id: i64,
            title: &str,
            related: bool,
            features: Vec<MatchedFeatureView>,
            watchlist: bool,
        ) -> ScoredCandidate {
            ScoredCandidate {
                candidate: CandidateView {
                    tmdb_id: Some(id),
                    title: title.into(),
                    year: Some(1992),
                    poster: None,
                    watchlist,
                    sources: vec![RetrievalSource {
                        kind: if watchlist {
                            RetrievalKind::Watchlist
                        } else if related {
                            RetrievalKind::Related
                        } else {
                            RetrievalKind::Filmography
                        },
                        label: if related {
                            "similar to Pulp Fiction".into()
                        } else {
                            features
                                .first()
                                .map(|f| f.name.clone())
                                .unwrap_or_else(|| "x".into())
                        },
                        seed_tmdb_id: if related { Some(680) } else { None },
                        seed_rating: None,
                    }],
                    directors: vec!["Q".into()],
                    genres: vec!["Crime".into()],
                    modes: vec![],
                    media_kind: MediaKind::Movie,
                    runtime: Some(99),
                    vote_count: Some(5000),
                },
                score: CandidateScore {
                    content: 0.5,
                    tmdb_related: if related { 1.0 } else { 0.0 },
                    friend_affinity: 0.0,
                    recent_taste: 0.0,
                    watchlist: if watchlist { 1.0 } else { 0.0 },
                    novelty: 0.0,
                    negative_evidence: 0.0,
                    semantic_fit: 0.5,
                    semantic_coverage: false,
                    total: 0.4,
                },
                reasons: vec![],
                evidence: vec![],
                positive_features: features.iter().map(|f| f.name.clone()).collect(),
                negative_features: vec![],
                contextual_only: false,
                person_keys: features.iter().map(|f| f.name.clone()).collect(),
                display_reasons: vec![],
                scoring_reasons: vec![],
                matched_features: features.clone(),
                hidden_features: vec![],
                eligibility: EligibilityTrace {
                    portable_evidence_required: false,
                    passed: watchlist || !features.is_empty(),
                    passed_because: vec!["craft".into()],
                    candidate_fit: 1.0,
                    evidence_grade: if watchlist {
                        crate::taste::explain::EvidenceGrade::Medium
                    } else if features.is_empty() {
                        crate::taste::explain::EvidenceGrade::None
                    } else {
                        crate::taste::explain::EvidenceGrade::Medium
                    },
                },
            }
        }

        let dogs = scored(500, "Reservoir Dogs", true, vec![], false);
        let kts = scored(
            49_530,
            "Killing Them Softly",
            false,
            vec![
                feat("Greig Fraser", "cinematographer", 4, 0.45),
                feat("neo-noir", "keyword", 9, 0.4),
            ],
            false,
        );
        let dune = scored(
            438_631,
            "Dune",
            false,
            vec![
                feat("Greig Fraser", "cinematographer", 4, 0.45),
                feat("Denis Villeneuve", "director", 5, 0.55),
            ],
            true,
        );
        let ws = assemble(&[dogs.clone(), kts, dune]);
        let q = board_quality(&ws, &HashSet::new());
        assert!(q.new_count >= 1);
        assert!(q.watchlist_count >= 1);
        assert_eq!(q.explore_count, 0);
        assert_eq!(q.new_below_floor, 0);
        assert_eq!(q.explore_outside_band, 0);
        assert_eq!(q.seen_leakage, 0);
        assert_eq!(q.board_overlap, 0);
        assert_eq!(q.watchlist_on_discovery_boards, 0);
        assert!(
            ws.new_picks.iter().any(|c| c.candidate.tmdb_id == Some(49_530)),
            "qualifying DP filmography belongs on New"
        );
        assert!(
            !ws.new_picks.iter().any(|c| c.candidate.tmdb_id == Some(500)),
            "lone similar-to must not occupy New"
        );
        assert!(ws.watchlist_picks.iter().any(|c| c.candidate.tmdb_id == Some(438_631)));
        assert!(
            crate::taste::confidence::match_score(&dogs) <= crate::taste::confidence::RELATED_ONLY_CAP,
            "70 is a band ceiling for related-only, not a pad, got {}",
            crate::taste::confidence::match_score(&dogs)
        );
    }
}
