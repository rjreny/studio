//! Snapshot of a real library for Taste profile regressions.
//! Written by `inspect_real_library` when `TASTE_WRITE_FIXTURE=1`.

use crate::taste::features::{Credit, Keyword};
use crate::taste::retrieve::FilmRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFilm {
    pub title: String,
    pub year: Option<i32>,
    pub tmdb_id: Option<i64>,
    pub rating: Option<f32>,
    pub liked: bool,
    pub watched: bool,
    pub watchlist: bool,
    pub viewings: u32,
    pub last_date: Option<String>,
    pub genres: Vec<String>,
    pub credits: Vec<Credit>,
    pub keywords: Vec<Keyword>,
    pub runtime: Option<i32>,
    pub age_years: Option<f32>,
}

pub fn snapshot_from_record(film: &FilmRecord) -> SnapshotFilm {
    SnapshotFilm {
        title: film.title.clone(),
        year: film.year,
        tmdb_id: film.tmdb_id,
        rating: film.rating,
        liked: film.liked,
        watched: film.watched,
        watchlist: film.watchlist,
        viewings: film.viewings,
        last_date: film.last_date.clone(),
        genres: film.genres.clone(),
        credits: film.credits.clone(),
        keywords: film.keywords.clone(),
        runtime: film.runtime,
        age_years: film.age_years,
    }
}

pub fn record_from_snapshot(film: &SnapshotFilm) -> FilmRecord {
    FilmRecord {
        key: film
            .tmdb_id
            .map(|id| format!("tmdb:{id}"))
            .unwrap_or_else(|| {
                format!(
                    "{}|{}",
                    film.title.trim().to_lowercase(),
                    film.year.unwrap_or(0)
                )
            }),
        title: film.title.clone(),
        year: film.year,
        tmdb_id: film.tmdb_id,
        rating: film.rating,
        liked: film.liked,
        watched: film.watched,
        watchlist: film.watchlist,
        viewings: film.viewings,
        last_date: film.last_date.clone(),
        genres: film.genres.clone(),
        credits: film.credits.clone(),
        keywords: film.keywords.clone(),
        similar: Vec::new(),
        runtime: film.runtime,
        poster: None,
        vote_count: None,
        signal: None,
        age_years: film.age_years,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::features::{keyword_role, FeatureFamily, KeywordRole};
    use crate::taste::retrieve::attach_signals;
    use crate::taste::feature_profile_from_films;

    #[test]
    fn library_627_profile_invariants() {
        let raw = include_str!("fixtures/library_627.json");
        let snaps: Vec<SnapshotFilm> =
            serde_json::from_str(raw).expect("library_627.json");
        let mut films: Vec<_> = snaps.iter().map(record_from_snapshot).collect();
        let rated = films.iter().filter(|f| f.rating.is_some()).count();
        assert!(
            rated >= 600,
            "fixture should be the real 627-film library, got {rated}"
        );
        attach_signals(&mut films);
        let profile = feature_profile_from_films(&films);

        let hill = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Stephen Hillenburg");
        if let Some(a) = hill {
            assert!(!a.citeable(), "Hillenburg n={}", a.appearances);
            assert_eq!(a.appearances, 1);
        }

        let fraser = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .expect("Fraser");
        assert!(fraser.citeable());
        assert!(fraser.appearances >= 2);

        let powell = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "John Powell")
            .expect("Powell");
        assert!(powell.citeable());
        assert!(powell.appearances >= 2);
        assert!(
            powell
                .evidence_cluster
                .genres
                .iter()
                .any(|g| g == "comedy" || g == "animation" || g == "family"),
            "{:?}",
            powell.evidence_cluster
        );

        let decade = profile
            .affinities
            .iter()
            .find(|a| a.key.family == FeatureFamily::Decade && a.key.name == "2000s")
            .expect("2000s");
        assert!(!decade.citeable());

        for name in ["new york city", "los angeles, california", "cartoon", "anti hero"] {
            if let Some(a) = profile.affinities.iter().find(|a| a.key.name == name) {
                assert!(!a.citeable(), "{name} must stay contextual");
                assert_eq!(keyword_role(name), KeywordRole::Contextual);
            }
        }
        assert_eq!(keyword_role("duringcreditsstinger"), KeywordRole::Ignore);
        assert_eq!(keyword_role("based on novel or book"), KeywordRole::Ignore);
        assert_eq!(keyword_role("neo-noir"), KeywordRole::Signal);
        assert_eq!(keyword_role("long take"), KeywordRole::Signal);
        assert_eq!(keyword_role("heist"), KeywordRole::Signal);

        let modes: Vec<_> = profile.modes.iter().map(|m| m.dimension.as_str()).collect();
        for need in ["story", "visual", "comedy", "spectacle", "intensity", "atmosphere"] {
            assert!(
                modes.iter().any(|m| *m == need),
                "missing mode {need} in {modes:?}"
            );
        }
    }

    fn watchlist_candidate(film: &SnapshotFilm) -> crate::taste::retrieve::Candidate {
        use crate::taste::retrieve::{Candidate, RetrievalKind, RetrievalSource};
        Candidate {
            tmdb_id: film.tmdb_id,
            title: film.title.clone(),
            year: film.year,
            poster: None,
            genres: film.genres.clone(),
            credits: film.credits.clone(),
            keywords: film.keywords.clone(),
            runtime: film.runtime,
            vote_count: None,
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

    fn is_watchlist_genre_only(c: &crate::taste::score::ScoredCandidate) -> bool {
        c.candidate.watchlist && c.person_keys.is_empty() && c.evidence.is_empty()
    }

    /// Real 627-film run: 31/50 were watchlist + Drama with empty evidence.
    #[test]
    fn library_627_watchlist_genre_only_cannot_dominate_shortlist() {
        use crate::taste::score::score_all;
        use crate::taste::shortlist::shortlist;
        let raw = include_str!("fixtures/library_627.json");
        let snaps: Vec<SnapshotFilm> =
            serde_json::from_str(raw).expect("library_627.json");
        let mut films: Vec<_> = snaps.iter().map(record_from_snapshot).collect();
        attach_signals(&mut films);
        let profile = feature_profile_from_films(&films);

        let mut cands: Vec<_> = snaps
            .iter()
            .filter(|f| f.watchlist && !f.watched)
            .map(watchlist_candidate)
            .collect();
        assert!(
            cands.len() >= 20,
            "fixture should include a real watchlist, got {}",
            cands.len()
        );

        let fraser = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .expect("Fraser");
        cands.push(watchlist_candidate(&SnapshotFilm {
            title: "The Gambler (fixture)".into(),
            year: Some(2014),
            tmdb_id: Some(9_000_001),
            rating: None,
            liked: false,
            watched: false,
            watchlist: true,
            viewings: 0,
            last_date: None,
            genres: vec!["Thriller".into(), "Crime".into(), "Drama".into()],
            credits: vec![Credit {
                id: fraser.key.id,
                name: "Greig Fraser".into(),
                job: "Director of Photography".into(),
            }],
            keywords: vec![],
            runtime: Some(111),
            age_years: None,
        }));

        let scored = score_all(&profile, &cands);
        let short = shortlist(&scored);
        let weak = short.iter().filter(|c| is_watchlist_genre_only(c)).count();
        assert_eq!(
            weak, 0,
            "watchlist+genre-only dominated the 627 shortlist: {weak}/{}",
            short.len()
        );
        assert!(
            scored
                .iter()
                .any(|c| c.candidate.title.contains("Gambler") && !c.contextual_only),
            "watchlist + Fraser must stay eligible on the real profile"
        );
        assert!(
            scored.iter().filter(|c| c.candidate.watchlist).all(|c| {
                crate::taste::score::reasons_have_strong_bridge(&c.reasons)
            }),
            "eligible watchlist titles must cite a person or keyword, not genre alone"
        );
        assert!(
            !scored.iter().any(|c| {
                c.candidate.title == "To Kill a Mockingbird" && !c.contextual_only
            }),
            "Mockingbird is watchlist+Drama-only"
        );
    }

    fn related_candidate(title: &str, genres: &[&str]) -> crate::taste::retrieve::Candidate {
        use crate::taste::retrieve::{Candidate, RetrievalKind, RetrievalSource};
        Candidate {
            tmdb_id: None,
            title: title.into(),
            year: Some(2012),
            poster: None,
            genres: genres.iter().map(|g| (*g).into()).collect(),
            credits: vec![],
            keywords: vec![],
            runtime: None,
            vote_count: Some(100),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Related,
                label: "similar to a liked film".into(),
                seed_tmdb_id: Some(1),
            }],
            friend_affinity: 0.0,
            tmdb_related: 1.0,
        }
    }

    /// Real 627-film run: 12/50 non-watchlist related rows cited only Drama/Comedy/Crime.
    #[test]
    fn library_627_related_genre_only_cannot_dominate_shortlist() {
        use crate::taste::score::score_all;
        use crate::taste::shortlist::shortlist;
        let raw = include_str!("fixtures/library_627.json");
        let snaps: Vec<SnapshotFilm> =
            serde_json::from_str(raw).expect("library_627.json");
        let mut films: Vec<_> = snaps.iter().map(record_from_snapshot).collect();
        attach_signals(&mut films);
        let profile = feature_profile_from_films(&films);

        let shelf = [
            ("A Late Quartet", &["Drama", "Music"][..]),
            ("Disaster Holiday", &["Family", "Comedy"][..]),
            ("Winged Creatures", &["Drama", "Crime"][..]),
            ("The Best of Me", &["Drama", "Romance"][..]),
            ("Fatherhood", &["Drama", "Comedy"][..]),
            ("Rise of the Footsoldier", &["Crime", "Thriller"][..]),
        ];
        let mut cands: Vec<_> = shelf
            .iter()
            .map(|(title, genres)| related_candidate(title, genres))
            .collect();
        let fraser = profile
            .affinities
            .iter()
            .find(|a| a.key.name == "Greig Fraser")
            .expect("Fraser");
        cands.push({
            let mut c = related_candidate("Dune (fixture)", &["Science Fiction"]);
            c.credits = vec![Credit {
                id: fraser.key.id,
                name: "Greig Fraser".into(),
                job: "Director of Photography".into(),
            }];
            c
        });
        let scored = score_all(&profile, &cands);
        let short = shortlist(&scored);
        let weak = short
            .iter()
            .filter(|c| {
                shelf.iter().any(|(title, _)| *title == c.candidate.title)
            })
            .count();
        assert_eq!(
            weak, 0,
            "related+genre-only dominated the 627 shortlist: {weak}/{}",
            short.len()
        );
        assert!(
            scored
                .iter()
                .any(|c| c.candidate.title.contains("Dune") && !c.contextual_only),
            "related + Fraser must stay eligible on the real profile"
        );
    }
}
