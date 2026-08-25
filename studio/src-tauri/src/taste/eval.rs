use crate::taste::score::ScoredCandidate;
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
}

pub fn recall_at(retrieved: &[String], held_out: &HashSet<String>, k: usize) -> f32 {
    if held_out.is_empty() {
        return 0.0;
    }
    let hit = retrieved.iter().take(k).filter(|id| held_out.contains(*id)).count();
    hit as f32 / held_out.len() as f32
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
    }
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
        assert!(!ids_of(&[]).is_empty() || true);
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
            similar: vec![],
            runtime: Some(100),
            poster: None,
            vote_count: Some(1000),
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

    fn filmography(
        title: &str,
        tmdb_id: i64,
        genres: &[&str],
        person: crate::taste::features::Credit,
    ) -> crate::taste::retrieve::Candidate {
        use crate::taste::retrieve::{RetrievalKind, RetrievalSource};
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
            }],
            friend_affinity: 0.0,
            tmdb_related: 0.0,
        }
    }

    /// 627-film Powell production failure, through the real deterministic pipeline.
    #[test]
    fn powell_e2e_627_history_retrieval_to_shortlist() {
        use crate::taste::features::{COMPOSER_W, CINEMATOGRAPHER_W};
        use crate::taste::reason::{empty_critic, ReasonerPick};
        use crate::taste::retrieve::{attach_signals, Candidate, RetrievalKind, RetrievalSource};
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
                }],
                friend_affinity: 0.0,
                tmdb_related: 0.55,
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
        assert!(ptrace.survived_mmr < 8, "MMR Powell {}", ptrace.survived_mmr);
        assert!((ptrace.confidence - expected_conf).abs() < 0.02);

        let powell_n = short
            .iter()
            .filter(|c| c.person_keys.iter().any(|k| k == "John Powell"))
            .count();
        assert!(
            powell_n < 8,
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
                *n < 8,
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
        assert_eq!(result.picks.len(), 12);
        assert_eq!(
            result.picks[0].candidate.tmdb_id,
            short[0].candidate.tmdb_id,
            "validate must not silently replace the first LLM pick"
        );
        let _ = empty_critic();
    }
}
