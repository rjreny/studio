use crate::catalog::tmdb;
use crate::letterboxd::posters::poster_url;
use crate::letterboxd::rss::parse_activity_payload;
use crate::models::LibraryItem;
use crate::storage::db::Database;
use crate::taste::features::{family_for_job, Credit, FeatureFamily, FeatureProfile, Keyword};
use crate::taste::preference::{
    interaction_signal, rating_profile, years_since, InteractionSignal, RatingProfile,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct FilmRecord {
    pub key: String,
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
    pub recommendations: Vec<LibraryItem>,
    pub similar: Vec<LibraryItem>,
    pub runtime: Option<i32>,
    pub poster: Option<String>,
    pub vote_count: Option<i64>,
    /// Optional personal Letterboxd review, distinct from TMDB review data.
    pub review: Option<String>,
    pub signal: Option<InteractionSignal>,
    pub age_years: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    #[default]
    Movie,
    TvSeries,
    TvEpisode,
    TvSpecial,
    Short,
    Other,
    Ambiguous,
}

impl MediaKind {
    pub fn is_movie(self) -> bool {
        matches!(self, MediaKind::Movie)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RetrievalKind {
    Related,
    RelatedRecommendations,
    RelatedSimilar,
    Filmography,
    Friend,
    Watchlist,
    Exploration,
    Discovery,
}

impl RetrievalKind {
    pub fn is_related(self) -> bool {
        matches!(
            self,
            Self::Related | Self::RelatedRecommendations | Self::RelatedSimilar
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalSource {
    pub kind: RetrievalKind,
    pub label: String,
    pub seed_tmdb_id: Option<i64>,
    #[serde(default)]
    pub seed_rating: Option<f32>,
}

impl RetrievalSource {
    pub fn new(kind: RetrievalKind, label: impl Into<String>, seed_tmdb_id: Option<i64>) -> Self {
        Self {
            kind,
            label: label.into(),
            seed_tmdb_id,
            seed_rating: None,
        }
    }

    pub fn with_rating(mut self, rating: Option<f32>) -> Self {
        self.seed_rating = rating;
        self
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedCoverage {
    pub eligible_seeds: usize,
    pub seeds_with_usable_related: usize,
    pub seeds_refreshed: usize,
    pub seeds_with_catalog: usize,
    pub candidates_with_catalog: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RetrievalResult {
    pub candidates: Vec<Candidate>,
    pub coverage: SeedCoverage,
}

const POOL_CAP: usize = 1000;
const PER_SEED_GUARANTEE: usize = 2;
const PER_PERSON_GUARANTEE: usize = 2;
const RECS_PER_SEED: usize = 8;
const SIMILAR_PER_SEED: usize = 3;
const SIMILAR_SEED_CAP: usize = 40;
const FILMOGRAPHY_PER_PERSON: usize = 12;
const ACTOR_FILMOGRAPHY_CAP: usize = 4;
const COMPOSER_FILMOGRAPHY_CAP: usize = 4;
const SEED_HYDRATE_CAP: usize = 40;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub tmdb_id: Option<i64>,
    pub title: String,
    pub year: Option<i32>,
    pub poster: Option<String>,
    pub genres: Vec<String>,
    pub credits: Vec<Credit>,
    pub keywords: Vec<Keyword>,
    pub runtime: Option<i32>,
    pub vote_count: Option<i64>,
    pub watchlist: bool,
    pub sources: Vec<RetrievalSource>,
    pub friend_affinity: f32,
    pub tmdb_related: f32,
    pub media_kind: MediaKind,
}

pub fn identity_key(tmdb_id: Option<i64>, title: &str, year: Option<i32>) -> String {
    if let Some(id) = tmdb_id {
        format!("tmdb:{id}")
    } else {
        format!("{}|{}", title.trim().to_lowercase(), year.unwrap_or(0))
    }
}

pub fn seen_keys(films: &[FilmRecord]) -> HashSet<String> {
    films
        .iter()
        .filter(|f| f.watched || f.rating.is_some())
        .map(|f| identity_key(f.tmdb_id, &f.title, f.year))
        .collect()
}

pub fn load_films(db: &Database) -> Result<Vec<FilmRecord>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT
              COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), '')),
              COALESCE(m.canonical_title, json_extract(smr.raw_identity, '$.title'), smr.normalized_title),
              COALESCE(m.release_year, smr.release_year),
              m.tmdb_id,
              ums.current_rating,
              COALESCE(ums.liked, 0),
              COALESCE(ums.watched, 0),
              COALESCE(ums.watchlist, smr.on_watchlist, 0),
              (SELECT COUNT(*) FROM viewings v WHERE v.source_movie_record_id = smr.id),
              COALESCE(ums.last_watched_at,
                (SELECT COALESCE(v.occurred_at, v.observed_at) FROM viewings v
                 WHERE v.source_movie_record_id = smr.id
                 ORDER BY COALESCE(v.occurred_at, v.observed_at) DESC LIMIT 1),
                (SELECT COALESCE(re.occurred_at, re.observed_at) FROM rating_events re
                 WHERE re.source_movie_record_id = smr.id
                 ORDER BY COALESCE(re.occurred_at, re.observed_at) DESC LIMIT 1)
              ),
              m.genres_json,
              m.credits_json,
              m.cast_json,
              m.crew_json,
              m.keywords_json,
              m.similar_json,
              m.runtime,
              COALESCE(m.poster_override_url, m.poster_path, smr.cached_poster_url),
              m.vote_count,
              smr.raw_identity
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
            WHERE ums.current_rating IS NOT NULL
               OR ums.watched = 1
               OR ums.watchlist = 1
               OR smr.on_watchlist = 1
               OR json_extract(smr.raw_identity, '$.review') IS NOT NULL
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let credits_json: Option<String> = row.get(11)?;
            let cast_json: Option<String> = row.get(12)?;
            let crew_json: Option<String> = row.get(13)?;
            let (recommendations, similar) = parse_related_lists(row.get::<_, Option<String>>(15)?);
            let review = row
                .get::<_, Option<String>>(19)?
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|identity| {
                    identity
                        .get("review")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .filter(|review| !review.trim().is_empty());
            Ok(FilmRecord {
                key: row.get(0)?,
                title: display_title(&row.get::<_, String>(1)?),
                year: row.get(2)?,
                tmdb_id: row.get(3)?,
                rating: row.get::<_, Option<f64>>(4)?.map(|r| r as f32),
                liked: row.get::<_, i32>(5)? == 1,
                watched: row.get::<_, i32>(6)? == 1,
                watchlist: row.get::<_, i32>(7)? == 1,
                viewings: row.get::<_, i64>(8)? as u32,
                last_date: row.get(9)?,
                genres: json_vec(row.get::<_, Option<String>>(10)?),
                credits: parse_credits(
                    credits_json.as_deref(),
                    cast_json.as_deref(),
                    crew_json.as_deref(),
                ),
                keywords: parse_keywords(row.get::<_, Option<String>>(14)?),
                recommendations,
                similar,
                runtime: row.get(16)?,
                poster: poster_url(row.get(17)?),
                vote_count: row.get(18)?,
                review,
                signal: None,
                age_years: None,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut films = consolidate_films(rows.filter_map(|r| r.ok()).collect());
    let now = chrono::Utc::now();
    for film in &mut films {
        if let Some(date) = film.last_date.as_deref() {
            film.age_years = years_since(date, now);
        }
    }
    Ok(films)
}

fn consolidate_films(films: Vec<FilmRecord>) -> Vec<FilmRecord> {
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut consolidated: Vec<FilmRecord> = Vec::new();

    for film in films {
        if let Some(&position) = positions.get(&film.key) {
            let current = &mut consolidated[position];
            let is_newer = film.last_date > current.last_date;
            current.watched |= film.watched;
            current.watchlist |= film.watchlist;
            current.liked |= film.liked;
            current.viewings += film.viewings;
            if is_newer {
                current.last_date = film.last_date;
                if film.rating.is_some() {
                    current.rating = film.rating;
                }
            } else if current.rating.is_none() {
                current.rating = film.rating;
            }
            if current.review.is_none() {
                current.review = film.review;
            }
        } else {
            positions.insert(film.key.clone(), consolidated.len());
            consolidated.push(film);
        }
    }
    consolidated
}

pub fn attach_signals(films: &mut [FilmRecord]) {
    let ratings: Vec<f32> = films.iter().filter_map(|f| f.rating).collect();
    let profile = rating_profile(&ratings);
    for film in films {
        if let (Some(rating), Some(profile)) = (film.rating, profile.as_ref()) {
            film.signal = Some(interaction_signal(
                rating,
                profile,
                film.age_years,
                film.viewings.max(1),
                film.liked,
            ));
        }
    }
}

fn needs_related_hydrate(film: &FilmRecord) -> bool {
    film.tmdb_id.is_some() && eligible_positive_like(film) && !has_usable_related(film)
}

pub fn enrich_eligible_seeds(
    db: &Database,
    films: &mut [FilmRecord],
    cap: usize,
    force: bool,
) -> usize {
    let cap = if cap == 0 { SEED_HYDRATE_CAP } else { cap };
    let mut idxs: Vec<usize> = films
        .iter()
        .enumerate()
        .filter(|(_, f)| eligible_positive_like(f) && f.tmdb_id.is_some())
        .filter(|(_, f)| force || needs_related_hydrate(f) || needs_catalog_hydrate(f))
        .map(|(i, _)| i)
        .collect();
    idxs.sort_by(|&a, &b| {
        let missing = |f: &FilmRecord| if needs_related_hydrate(f) { 0 } else { 1 };
        missing(&films[a])
            .cmp(&missing(&films[b]))
            .then_with(|| {
                seed_priority(&films[b])
                    .partial_cmp(&seed_priority(&films[a]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| films[a].tmdb_id.cmp(&films[b].tmdb_id))
            .then_with(|| films[a].title.cmp(&films[b].title))
    });
    let mut n = 0;
    for i in idxs.into_iter().take(cap) {
        let Some(tid) = films[i].tmdb_id else {
            continue;
        };
        if tmdb::refresh_movie_catalog(db, tid, force).is_ok() {
            n += 1;
            let _ = reload_catalog_fields(db, &mut films[i]);
        }
    }
    n
}

fn needs_catalog_hydrate(film: &FilmRecord) -> bool {
    if film.tmdb_id.is_none() || film.rating.is_none() {
        return false;
    }
    let has_crew_ids = film
        .credits
        .iter()
        .any(|c| c.id.is_some() && c.job != "Actor");
    !has_crew_ids || film.keywords.is_empty()
}

pub fn enrich_rated_library(
    db: &Database,
    films: &mut [FilmRecord],
    cap: usize,
    force: bool,
) -> usize {
    let mut idxs: Vec<usize> = films
        .iter()
        .enumerate()
        .filter(|(_, f)| force || needs_catalog_hydrate(f))
        .map(|(i, _)| i)
        .collect();
    idxs.sort_by(|&a, &b| {
        let score = |f: &FilmRecord| {
            let abs = f
                .rating
                .map(crate::taste::preference::absolute_preference)
                .unwrap_or(0.0)
                .abs();
            abs * crate::taste::preference::recency_weight(f.age_years)
        };
        score(&films[b])
            .partial_cmp(&score(&films[a]))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| films[a].tmdb_id.cmp(&films[b].tmdb_id))
            .then_with(|| films[a].title.cmp(&films[b].title))
    });
    let mut n = 0;
    for i in idxs.into_iter().take(cap) {
        let Some(tid) = films[i].tmdb_id else {
            continue;
        };
        if tmdb::refresh_movie_catalog(db, tid, force).is_ok() {
            n += 1;
            let _ = reload_catalog_fields(db, &mut films[i]);
        }
    }
    n
}

fn reload_catalog_fields(db: &Database, film: &mut FilmRecord) -> Result<(), String> {
    let Some(tid) = film.tmdb_id else {
        return Ok(());
    };
    let row = db.conn().query_row(
        "SELECT genres_json, credits_json, cast_json, crew_json, keywords_json, similar_json, runtime, vote_count, poster_path
         FROM movies WHERE tmdb_id = ?1 LIMIT 1",
        params![tid],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i32>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        },
    );
    let Ok((genres, credits, cast, crew, keywords, similar, runtime, vote_count, poster)) = row
    else {
        return Ok(());
    };
    film.genres = json_vec(genres);
    film.credits = parse_credits(credits.as_deref(), cast.as_deref(), crew.as_deref());
    film.keywords = parse_keywords(keywords);
    let (recommendations, similar_list) = parse_related_lists(similar);
    film.recommendations = recommendations;
    film.similar = similar_list;
    if film.runtime.is_none() {
        film.runtime = runtime;
    }
    if film.vote_count.is_none() {
        film.vote_count = vote_count;
    }
    if film.poster.is_none() {
        film.poster = poster_url(poster);
    }
    Ok(())
}

pub fn user_profile(films: &[FilmRecord]) -> Option<RatingProfile> {
    let ratings: Vec<f32> = films.iter().filter_map(|f| f.rating).collect();
    rating_profile(&ratings)
}

pub fn retrieve(
    db: &Database,
    films: &[FilmRecord],
    profile: &FeatureProfile,
    seen: &HashSet<String>,
    force_refresh: bool,
) -> Result<Vec<Candidate>, String> {
    Ok(retrieve_with_coverage(db, films, profile, seen, force_refresh)?.candidates)
}

pub fn retrieve_with_coverage(
    db: &Database,
    films: &[FilmRecord],
    profile: &FeatureProfile,
    seen: &HashSet<String>,
    force_refresh: bool,
) -> Result<RetrievalResult, String> {
    let mut by_key: HashMap<String, Candidate> = HashMap::new();
    let mut seeds: Vec<&FilmRecord> = films.iter().filter(|f| eligible_positive_like(f)).collect();
    seeds.sort_by(|a, b| {
        seed_priority(b)
            .partial_cmp(&seed_priority(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.tmdb_id
                    .unwrap_or(i64::MAX)
                    .cmp(&b.tmdb_id.unwrap_or(i64::MAX))
            })
            .then_with(|| a.title.cmp(&b.title))
    });

    let eligible_seeds = seeds.len();
    let seeds_with_usable_related = seeds.iter().filter(|s| has_usable_related(s)).count();

    for seed in &seeds {
        for item in seed.recommendations.iter().take(RECS_PER_SEED) {
            push_related(
                &mut by_key,
                seen,
                seed,
                item,
                RetrievalKind::RelatedRecommendations,
                format!("recommended from {}", seed.title),
            );
        }
    }
    for seed in seeds.iter().take(SIMILAR_SEED_CAP) {
        let allow_similar = seed_allows_similar(seed, profile);
        if !allow_similar {
            continue;
        }
        for item in seed.similar.iter().take(SIMILAR_PER_SEED) {
            push_related(
                &mut by_key,
                seen,
                seed,
                item,
                RetrievalKind::RelatedSimilar,
                format!("similar to {}", seed.title),
            );
        }
    }

    let people: Vec<_> = profile
        .affinities
        .iter()
        .filter(|a| filmography_person_allowed(a))
        .collect();
    for person in people {
        let Some(pid) = person.key.id else { continue };
        let credits =
            tmdb::person_movie_credits_with_force(db, pid, force_refresh).unwrap_or_default();
        let take = filmography_credit_cap(person.key.family);
        for credit in credits
            .into_iter()
            .filter(|c| keep_person_credit(&c.job, person.key.family))
            .take(take)
        {
            let key = identity_key(Some(credit.tmdb_id), &credit.title, credit.year);
            if seen.contains(&key) {
                continue;
            }
            upsert_candidate(
                &mut by_key,
                Candidate {
                    tmdb_id: Some(credit.tmdb_id),
                    title: credit.title,
                    year: credit.year,
                    poster: None,
                    genres: Vec::new(),
                    credits: vec![Credit {
                        id: Some(pid),
                        name: person.key.name.clone(),
                        job: credit.job,
                    }],
                    keywords: Vec::new(),
                    runtime: None,
                    vote_count: None,
                    watchlist: false,
                    sources: vec![RetrievalSource::new(
                        RetrievalKind::Filmography,
                        person.key.name.clone(),
                        None,
                    )],
                    friend_affinity: 0.0,
                    tmdb_related: 0.0,
                    media_kind: MediaKind::Movie,
                },
            );
        }
    }

    for film in films.iter().filter(|f| f.watchlist && !f.watched) {
        let key = identity_key(film.tmdb_id, &film.title, film.year);
        if seen.contains(&key) {
            continue;
        }
        upsert_candidate(
            &mut by_key,
            Candidate {
                tmdb_id: film.tmdb_id,
                title: film.title.clone(),
                year: film.year,
                poster: film.poster.clone(),
                genres: film.genres.clone(),
                credits: film.credits.clone(),
                keywords: film.keywords.clone(),
                runtime: film.runtime,
                vote_count: film.vote_count,
                watchlist: true,
                sources: vec![RetrievalSource {
                    kind: RetrievalKind::Watchlist,
                    label: "watchlist".into(),
                    seed_tmdb_id: None,
                    seed_rating: None,
                }],
                friend_affinity: 0.0,
                tmdb_related: 0.0,
                media_kind: MediaKind::Movie,
            },
        );
    }

    let friend_hits = friend_candidates(db, films, seen)?;
    for c in friend_hits {
        upsert_candidate(&mut by_key, c);
    }

    let mut out = select_fair_pool(by_key, POOL_CAP);
    hydrate_local_metadata(db, &mut out)?;
    let seeds_with_catalog = seeds.iter().filter(|s| film_has_catalog(s)).count();
    let candidates_with_catalog = out.iter().filter(|c| candidate_has_catalog(c)).count();
    Ok(RetrievalResult {
        candidates: out,
        coverage: SeedCoverage {
            eligible_seeds,
            seeds_with_usable_related,
            seeds_refreshed: 0,
            seeds_with_catalog,
            candidates_with_catalog,
        },
    })
}

fn push_related(
    by_key: &mut HashMap<String, Candidate>,
    seen: &HashSet<String>,
    seed: &FilmRecord,
    item: &LibraryItem,
    kind: RetrievalKind,
    label: String,
) {
    let tmdb_id = tmdb_id_from_item(item);
    let key = identity_key(tmdb_id, &item.title, item.year);
    if seen.contains(&key) {
        return;
    }
    upsert_candidate(
        by_key,
        Candidate {
            tmdb_id,
            title: item.title.clone(),
            year: item.year,
            poster: item.poster.clone(),
            genres: Vec::new(),
            credits: Vec::new(),
            keywords: Vec::new(),
            runtime: None,
            vote_count: None,
            watchlist: false,
            sources: vec![RetrievalSource::new(kind, label, seed.tmdb_id).with_rating(seed.rating)],
            friend_affinity: 0.0,
            tmdb_related: 1.0,
            media_kind: MediaKind::Movie,
        },
    );
}

fn seed_priority(seed: &FilmRecord) -> f32 {
    seed.signal
        .as_ref()
        .map(|s| s.preference.affinity_preference * s.recommendation_weight)
        .unwrap_or(0.0)
}

fn has_usable_related(seed: &FilmRecord) -> bool {
    !seed.recommendations.is_empty() || !seed.similar.is_empty()
}

pub fn eligible_positive_like(seed: &FilmRecord) -> bool {
    let Some(signal) = seed.signal.as_ref() else {
        return false;
    };
    if seed.rating.is_none() {
        return false;
    }
    if signal.preference.affinity_preference <= 0.0 || signal.preference.absolute <= 0.0 {
        return false;
    }
    if signal.familiarity_strength >= 0.6 {
        return false;
    }
    if seed
        .genres
        .iter()
        .any(|g| g.eq_ignore_ascii_case("tv movie"))
    {
        return false;
    }
    true
}

/// Related expansion is for every eligible positive-like film. The source
/// provenance and later evidence grade decide whether a result is displayed.
fn seed_expands_related(seed: &FilmRecord, _profile: &FeatureProfile) -> bool {
    eligible_positive_like(seed)
}

fn seed_allows_similar(seed: &FilmRecord, _profile: &FeatureProfile) -> bool {
    eligible_positive_like(seed)
}

fn filmography_person_allowed(a: &crate::taste::features::FeatureAffinity) -> bool {
    if a.key.id.is_none() || !a.citeable() {
        return false;
    }
    match a.key.family {
        FeatureFamily::Director | FeatureFamily::Writer | FeatureFamily::Cinematographer => {
            a.recommendation_mean > 0.15
        }
        FeatureFamily::Actor => {
            a.appearances >= 3
                && a.recommendation_mean >= crate::taste::features::PORTABLE_CONTEXTUAL
        }
        FeatureFamily::Composer => a.appearances >= 4 && a.recommendation_mean > 0.22,
        _ => false,
    }
}

fn filmography_credit_cap(family: FeatureFamily) -> usize {
    match family {
        FeatureFamily::Actor => ACTOR_FILMOGRAPHY_CAP,
        FeatureFamily::Composer => COMPOSER_FILMOGRAPHY_CAP,
        _ => FILMOGRAPHY_PER_PERSON,
    }
}

fn film_has_catalog(film: &FilmRecord) -> bool {
    !film.genres.is_empty()
        && film.credits.iter().any(|c| c.id.is_some())
        && !film.keywords.is_empty()
        && (!film.recommendations.is_empty() || !film.similar.is_empty())
}

fn candidate_has_catalog(c: &Candidate) -> bool {
    !c.genres.is_empty()
        && c.credits.iter().any(|cr| cr.id.is_some())
        && (!c.keywords.is_empty() || c.runtime.is_some())
}

fn candidate_priority(c: &Candidate) -> i32 {
    if c.watchlist {
        return 500;
    }
    let mut p = 0;
    for s in &c.sources {
        p += match s.kind {
            RetrievalKind::Friend => 40,
            RetrievalKind::RelatedRecommendations | RetrievalKind::Related => 10,
            RetrievalKind::RelatedSimilar => 3,
            RetrievalKind::Filmography => 1,
            RetrievalKind::Watchlist => 50,
            RetrievalKind::Discovery | RetrievalKind::Exploration => 2,
        };
    }
    p + (c.sources.len() as i32)
}

fn pool_order(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    candidate_priority(a)
        .cmp(&candidate_priority(b))
        .then(a.sources.len().cmp(&b.sources.len()))
        .then(a.tmdb_id.unwrap_or(0).cmp(&b.tmdb_id.unwrap_or(0)))
        .then(a.title.cmp(&b.title))
}

fn select_fair_pool(map: HashMap<String, Candidate>, cap: usize) -> Vec<Candidate> {
    if map.len() <= cap {
        let mut out: Vec<_> = map.into_values().collect();
        out.sort_by(|a, b| pool_order(b, a));
        return out;
    }

    let mut by_seed: HashMap<i64, Vec<String>> = HashMap::new();
    let mut by_person: HashMap<String, Vec<String>> = HashMap::new();
    let mut must: Vec<String> = Vec::new();
    for (key, c) in &map {
        if c.watchlist || c.sources.iter().any(|s| s.kind == RetrievalKind::Friend) {
            must.push(key.clone());
        }
        for s in &c.sources {
            if s.kind.is_related() {
                if let Some(seed) = s.seed_tmdb_id {
                    by_seed.entry(seed).or_default().push(key.clone());
                }
            }
            if s.kind == RetrievalKind::Filmography {
                by_person
                    .entry(s.label.clone())
                    .or_default()
                    .push(key.clone());
            }
        }
    }
    for keys in by_seed.values_mut() {
        keys.sort();
        keys.dedup();
        keys.sort_by(|a, b| pool_order(&map[b], &map[a]));
    }
    for keys in by_person.values_mut() {
        keys.sort();
        keys.dedup();
        keys.sort_by(|a, b| pool_order(&map[b], &map[a]));
    }

    let mut selected: Vec<String> = Vec::new();
    let mut seen_sel = HashSet::new();
    let mut push_key = |key: String, selected: &mut Vec<String>| {
        if selected.len() >= cap {
            return;
        }
        if seen_sel.insert(key.clone()) {
            selected.push(key);
        }
    };

    must.sort();
    for key in must {
        push_key(key, &mut selected);
    }

    let mut seed_ids: Vec<i64> = by_seed.keys().copied().collect();
    seed_ids.sort();
    for pass in 0..PER_SEED_GUARANTEE {
        for sid in &seed_ids {
            if let Some(keys) = by_seed.get(sid) {
                if let Some(key) = keys.get(pass) {
                    push_key(key.clone(), &mut selected);
                }
            }
        }
    }
    let mut people: Vec<String> = by_person.keys().cloned().collect();
    people.sort();
    for pass in 0..PER_PERSON_GUARANTEE {
        for name in &people {
            if let Some(keys) = by_person.get(name) {
                if let Some(key) = keys.get(pass) {
                    push_key(key.clone(), &mut selected);
                }
            }
        }
    }

    let mut rest: Vec<String> = map.keys().cloned().collect();
    rest.sort_by(|a, b| pool_order(&map[b], &map[a]).then(a.cmp(b)));
    for key in rest {
        push_key(key, &mut selected);
    }

    selected
        .into_iter()
        .filter_map(|k| map.get(&k).cloned())
        .collect()
}

pub(crate) fn keep_person_credit(job: &str, family: FeatureFamily) -> bool {
    family_for_job(job) == Some(family)
}

fn upsert_candidate(map: &mut HashMap<String, Candidate>, incoming: Candidate) {
    if !incoming.media_kind.is_movie() {
        return;
    }
    let key = identity_key(incoming.tmdb_id, &incoming.title, incoming.year);
    map.entry(key)
        .and_modify(|existing| {
            for src in incoming.sources.clone() {
                if !existing.sources.iter().any(|s| {
                    s.kind == src.kind && s.seed_tmdb_id == src.seed_tmdb_id && s.label == src.label
                }) {
                    existing.sources.push(src);
                }
            }
            existing.tmdb_related = existing.tmdb_related.max(incoming.tmdb_related);
            existing.friend_affinity += incoming.friend_affinity;
            existing.watchlist |= incoming.watchlist;
            if existing.credits.is_empty() {
                existing.credits = incoming.credits.clone();
            }
            if existing.genres.is_empty() {
                existing.genres = incoming.genres.clone();
            }
            if existing.poster.is_none() {
                existing.poster = incoming.poster.clone();
            }
            if existing.tmdb_id.is_none() {
                existing.tmdb_id = incoming.tmdb_id;
            }
        })
        .or_insert(incoming);
}

fn tmdb_id_from_item(item: &LibraryItem) -> Option<i64> {
    item.id.strip_prefix("tmdb:").and_then(|s| s.parse().ok())
}

fn friend_candidates(
    db: &Database,
    films: &[FilmRecord],
    seen: &HashSet<String>,
) -> Result<Vec<Candidate>, String> {
    let user_by_tmdb: HashMap<i64, f32> = films
        .iter()
        .filter_map(|f| Some((f.tmdb_id?, f.rating?)))
        .collect();
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT f.id, f.username, fa.rating, fa.raw_payload, m.tmdb_id,
                   COALESCE(m.canonical_title, json_extract(smr.raw_identity, '$.title'))
            FROM friend_activity fa
            JOIN friends f ON f.id = fa.friend_id
            LEFT JOIN source_movie_records smr ON smr.id = fa.source_movie_record_id
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            WHERE fa.rating IS NOT NULL AND fa.rating >= 3.5
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    #[derive(Default)]
    struct FriendAcc {
        username: String,
        pairs: Vec<(f32, f32)>,
        ratings: Vec<f32>,
    }
    let mut friends: HashMap<String, FriendAcc> = HashMap::new();
    let mut movie_friends: HashMap<String, Vec<(String, f32, f32)>> = HashMap::new();

    struct Row {
        friend_id: String,
        username: String,
        rating: f32,
        tmdb_id: Option<i64>,
        title: String,
        year: Option<i32>,
    }
    let mut parsed = Vec::new();
    for row in rows.flatten() {
        let (friend_id, username, rating, raw, tmdb_id, title_opt) = row;
        let Some(rating) = rating.map(|r| r as f32) else {
            continue;
        };
        let (parsed_title, year) = parse_activity_payload(&raw);
        let title = title_opt.filter(|s| !s.is_empty()).unwrap_or(parsed_title);
        let tmdb_id = tmdb_id.or_else(|| tmdb_id_from_raw(&raw));
        parsed.push(Row {
            friend_id,
            username,
            rating,
            tmdb_id,
            title,
            year,
        });
    }

    for row in &parsed {
        let acc = friends.entry(row.friend_id.clone()).or_insert(FriendAcc {
            username: row.username.clone(),
            ..Default::default()
        });
        acc.ratings.push(row.rating);
        if let Some(tid) = row.tmdb_id {
            if let Some(mine) = user_by_tmdb.get(&tid) {
                acc.pairs.push((*mine, row.rating));
            }
        }
    }

    let mut similarity: HashMap<String, f32> = HashMap::new();
    for (id, acc) in &friends {
        similarity.insert(id.clone(), friend_similarity(&acc.pairs, &acc.ratings));
    }

    for row in &parsed {
        if row.rating < 3.5 {
            continue;
        }
        let key = identity_key(row.tmdb_id, &row.title, row.year);
        if seen.contains(&key) {
            continue;
        }
        let sim = similarity.get(&row.friend_id).copied().unwrap_or(0.0);
        movie_friends
            .entry(key)
            .or_default()
            .push((row.username.clone(), row.rating, sim));
    }

    let mut out = Vec::new();
    for (_key, contribs) in movie_friends {
        let first = parsed
            .iter()
            .find(|r| identity_key(r.tmdb_id, &r.title, r.year) == _key);
        let Some(sample) = first else { continue };
        let friend_affinity = contribs
            .iter()
            .map(|(_, rating, sim)| ((rating - 3.0) / 2.0).clamp(-1.0, 1.0) * sim)
            .sum::<f32>()
            .clamp(-1.0, 1.0);
        out.push(Candidate {
            tmdb_id: sample.tmdb_id,
            title: sample.title.clone(),
            year: sample.year,
            poster: None,
            genres: Vec::new(),
            credits: Vec::new(),
            keywords: Vec::new(),
            runtime: None,
            vote_count: None,
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::Friend,
                label: format!("loved by {} friend(s)", contribs.len()),
                seed_tmdb_id: None,
                seed_rating: None,
            }],
            friend_affinity,
            tmdb_related: 0.0,
            media_kind: MediaKind::Movie,
        });
    }
    Ok(out)
}

pub fn friend_identity_keys(db: &Database) -> Vec<String> {
    let mut stmt = match db
        .conn()
        .prepare("SELECT source_record_key FROM friend_activity")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |row| row.get::<_, String>(0))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

fn friend_similarity(pairs: &[(f32, f32)], _ratings: &[f32]) -> f32 {
    let n = pairs.len();
    if n < 5 {
        return 0.0;
    }
    let mean_a = pairs.iter().map(|(a, _)| a).sum::<f32>() / n as f32;
    let mean_b = pairs.iter().map(|(_, b)| b).sum::<f32>() / n as f32;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for (a, b) in pairs {
        let za = a - mean_a;
        let zb = b - mean_b;
        num += za * zb;
        da += za * za;
        db += zb * zb;
    }
    if da <= 1e-6 || db <= 1e-6 {
        return 0.0;
    }
    let corr = (num / (da.sqrt() * db.sqrt())).clamp(-1.0, 1.0);
    let conf = 1.0 - (-(n as f32) / 8.0).exp();
    corr * conf
}

fn tmdb_id_from_raw(raw: &str) -> Option<i64> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(id) = v.get("tmdb_id").and_then(|x| x.as_i64()) {
            return Some(id);
        }
    }
    None
}

fn hydrate_local_metadata(db: &Database, candidates: &mut [Candidate]) -> Result<(), String> {
    for c in candidates.iter_mut() {
        let Some(tid) = c.tmdb_id else { continue };
        let row = db.conn().query_row(
            "SELECT genres_json, credits_json, cast_json, crew_json, keywords_json, runtime, vote_count, poster_path, canonical_title, release_year
             FROM movies WHERE tmdb_id = ?1",
            params![tid],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i32>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<i32>>(9)?,
                ))
            },
        );
        if let Ok((
            genres,
            credits,
            cast,
            crew,
            keywords,
            runtime,
            vote_count,
            poster,
            title,
            year,
        )) = row
        {
            if c.genres.is_empty() {
                c.genres = json_vec(genres);
            }
            let parsed = parse_credits(credits.as_deref(), cast.as_deref(), crew.as_deref());
            if !parsed.is_empty() {
                c.credits = parsed;
            }
            if c.keywords.is_empty() {
                c.keywords = parse_keywords(keywords);
            }
            if c.runtime.is_none() {
                c.runtime = runtime;
            }
            if c.vote_count.is_none() {
                c.vote_count = vote_count;
            }
            if c.poster.is_none() {
                c.poster = poster_url(poster);
            }
            if let Some(t) = title {
                if !t.is_empty() {
                    c.title = t;
                }
            }
            if c.year.is_none() {
                c.year = year;
            }
        }
    }
    Ok(())
}

pub fn enrich_missing(
    db: &Database,
    candidates: &mut [Candidate],
    cap: usize,
    force: bool,
) -> usize {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| {
        let rank = |c: &Candidate| {
            let recs = c
                .sources
                .iter()
                .any(|s| s.kind == RetrievalKind::RelatedRecommendations);
            let needs = c.genres.is_empty() || c.credits.len() <= 1 || c.keywords.is_empty();
            // Explicit watchlist intent must get metadata before the much
            // larger related/recommendation pool. Otherwise a small hydrate
            // budget can silently leave saved titles without the credits or
            // keywords needed by the watchlist bridge.
            (!c.watchlist, !recs, !needs)
        };
        rank(&candidates[a]).cmp(&rank(&candidates[b]))
    });
    let mut n = 0;
    for i in order {
        if n >= cap {
            break;
        }
        let c = &mut candidates[i];
        let Some(tid) = c.tmdb_id else { continue };
        if !force && !c.genres.is_empty() && c.credits.len() > 1 && !c.keywords.is_empty() {
            continue;
        }
        if tmdb::refresh_movie_catalog(db, tid, force).is_ok() {
            n += 1;
        }
    }
    let _ = hydrate_local_metadata(db, candidates);
    n
}

fn display_title(raw: &str) -> String {
    let (title, _) = parse_activity_payload(raw);
    if title.trim().is_empty() {
        raw.to_string()
    } else {
        title
    }
}

fn json_vec(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn parse_keywords(raw: Option<String>) -> Vec<Keyword> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<serde_json::Value>>(&raw)
        .ok()
        .map(|vals| {
            vals.iter()
                .filter_map(|v| {
                    Some(Keyword {
                        id: v["id"].as_i64(),
                        name: v["name"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_credits(credits: Option<&str>, cast: Option<&str>, crew: Option<&str>) -> Vec<Credit> {
    if let Some(raw) = credits {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
            let mut out = Vec::new();
            if let Some(arr) = v["crew"].as_array() {
                for c in arr {
                    if let Some(name) = c["name"].as_str() {
                        out.push(Credit {
                            id: c["id"].as_i64(),
                            name: name.to_string(),
                            job: c["job"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
            }
            if let Some(arr) = v["cast"].as_array() {
                for c in arr.iter().take(8) {
                    if let Some(name) = c["name"].as_str() {
                        out.push(Credit {
                            id: c["id"].as_i64(),
                            name: name.to_string(),
                            job: "Actor".into(),
                        });
                    }
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    let mut out = Vec::new();
    if let Some(crew) = crew {
        if let Ok(names) = serde_json::from_str::<Vec<String>>(crew) {
            for entry in names {
                if let Some((name, job)) = entry.rsplit_once(" (") {
                    out.push(Credit {
                        id: None,
                        name: name.to_string(),
                        job: job.trim_end_matches(')').to_string(),
                    });
                }
            }
        }
    }
    if let Some(cast) = cast {
        if let Ok(names) = serde_json::from_str::<Vec<String>>(cast) {
            for entry in names.into_iter().take(8) {
                let name = entry.split(" as ").next().unwrap_or(&entry).trim();
                if !name.is_empty() {
                    out.push(Credit {
                        id: None,
                        name: name.to_string(),
                        job: "Actor".into(),
                    });
                }
            }
        }
    }
    out
}

fn parse_related_lists(raw: Option<String>) -> (Vec<LibraryItem>, Vec<LibraryItem>) {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return (Vec::new(), Vec::new());
    };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
        if let Some(obj) = v.as_object() {
            if obj.contains_key("recommendations") || obj.contains_key("similar") {
                return (
                    parse_item_list(obj.get("recommendations")),
                    parse_item_list(obj.get("similar")),
                );
            }
        }
        if v.as_array().is_some() {
            let items = parse_item_list(Some(&v));
            return (items, Vec::new());
        }
    }
    (Vec::new(), Vec::new())
}

fn parse_item_list(v: Option<&serde_json::Value>) -> Vec<LibraryItem> {
    let Some(v) = v else {
        return Vec::new();
    };
    if let Ok(items) = serde_json::from_value::<Vec<LibraryItem>>(v.clone()) {
        if items.iter().any(|item| !item.title.is_empty()) {
            return items;
        }
    }
    v.as_array()
        .map(|vals| {
            vals.iter()
                .filter_map(tmdb::library_item_from_movie_value)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_similar(raw: Option<String>) -> Vec<LibraryItem> {
    let (recs, similar) = parse_related_lists(raw);
    let mut out = recs;
    out.extend(similar);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friend_overlap_gate() {
        assert_eq!(
            friend_similarity(&[(5.0, 5.0), (4.0, 4.0)], &[5.0, 4.0]),
            0.0
        );
        let pairs: Vec<(f32, f32)> = (0..20)
            .map(|i| (3.0 + (i % 3) as f32 * 0.5, 3.0 + (i % 3) as f32 * 0.5))
            .collect();
        assert!(friend_similarity(&pairs, &[]).abs() > 0.3);
    }

    #[test]
    fn parse_related_lists_keeps_provenance() {
        let tagged = serde_json::json!({
            "recommendations": [{"id":"tmdb:1","title":"Rec","year":2020}],
            "similar": [{"id":"tmdb:2","title":"Sim","year":2019}]
        });
        let (recs, similar) = parse_related_lists(Some(tagged.to_string()));
        assert_eq!(recs[0].title, "Rec");
        assert_eq!(similar[0].title, "Sim");
        let legacy = serde_json::json!([{"id":"tmdb:3","title":"Old","year":2010}]);
        let (recs, similar) = parse_related_lists(Some(legacy.to_string()));
        assert_eq!(recs[0].title, "Old");
        assert!(similar.is_empty());
    }

    #[test]
    fn identity_prefers_tmdb() {
        assert_eq!(identity_key(Some(99), "Heat", Some(1995)), "tmdb:99");
        assert_eq!(identity_key(None, "Heat", Some(1995)), "heat|1995");
    }

    #[test]
    fn consolidates_duplicate_source_records() {
        let record = |viewings, rating| FilmRecord {
            key: "tmdb:1571662".into(),
            title: "Tuner".into(),
            year: Some(2025),
            tmdb_id: Some(1571662),
            rating,
            liked: false,
            watched: true,
            watchlist: false,
            viewings,
            last_date: Some("2025-01-01".into()),
            genres: vec![],
            credits: vec![],
            keywords: vec![],
            recommendations: vec![],
            similar: vec![],
            runtime: None,
            poster: None,
            vote_count: None,
            review: None,
            signal: None,
            age_years: None,
        };
        let films = consolidate_films(vec![record(1, None), record(0, Some(4.5))]);
        assert_eq!(films.len(), 1);
        assert_eq!(films[0].viewings, 1);
        assert_eq!(films[0].rating, Some(4.5));
    }

    #[test]
    fn seed_rank_ignores_rewatch_boost() {
        use crate::taste::preference::{interaction_signal, rating_profile};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let once = interaction_signal(4.5, &p, Some(1.0), 1, false);
        let many = interaction_signal(4.5, &p, Some(1.0), 7, false);
        let once_seed = once.preference.absolute * once.recommendation_weight;
        let many_seed = many.preference.absolute * many.recommendation_weight;
        assert!(many_seed < once_seed);
        assert!(many.preference_weight > once.preference_weight);
    }

    fn test_seed(title: &str, viewings: u32, credits: Vec<Credit>) -> FilmRecord {
        use crate::taste::preference::{interaction_signal, rating_profile};
        let p = rating_profile(&[4.0; 8]).unwrap();
        FilmRecord {
            key: title.into(),
            title: title.into(),
            year: Some(2011),
            tmdb_id: Some(1),
            rating: Some(5.0),
            liked: true,
            watched: true,
            watchlist: false,
            viewings,
            last_date: None,
            genres: vec!["Drama".into()],
            credits,
            keywords: vec![],
            recommendations: vec![LibraryItem::catalog(
                "tmdb:99".into(),
                "Dirty".into(),
                Some(2005),
                None,
                None,
                None,
            )],
            similar: vec![],
            runtime: None,
            poster: None,
            vote_count: None,
            review: None,
            signal: Some(interaction_signal(5.0, &p, Some(0.4), viewings, false)),
            age_years: Some(0.4),
        }
    }

    #[test]
    fn twilight_positive_like_expands_without_citeable_director() {
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let condon = Credit {
            id: Some(99),
            name: "Bill Condon".into(),
            job: "Director".into(),
        };
        let obs = observations_from_film(
            "The Twilight Saga: Breaking Dawn - Part 1",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Drama".into()],
            &[condon.clone()],
            &[],
            Some(2011),
            None,
        );
        let profile = build_profile(&obs);
        let seed = test_seed("Twilight", 1, vec![condon]);
        assert!(
            seed_expands_related(&seed, &profile),
            "an eligible positive-like film expands even when its director is not citeable"
        );
    }

    #[test]
    fn citeable_person_seed_does_expand_related() {
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
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
            &interaction_signal(4.5, &p, Some(0.2), 1, false),
            Some(0.2),
            &["Science Fiction".into()],
            &[dp.clone()],
            &[],
            Some(2021),
            None,
        ));
        let profile = build_profile(&obs);
        let seed = test_seed("The Batman", 1, vec![dp.clone()]);
        assert!(seed_expands_related(&seed, &profile));
        let rewatch = test_seed("The Batman", 7, vec![dp.clone()]);
        assert!(
            !seed_expands_related(&rewatch, &profile),
            "familiar films must not flood related"
        );
        let mut family = test_seed("Teen Beach 2", 1, vec![dp]);
        family.tmdb_id = Some(3);
        family.genres = vec!["Family".into(), "Comedy".into()];
        assert!(
            seed_expands_related(&family, &profile),
            "family/kids seeds may expand; display still requires Medium evidence"
        );
    }

    #[test]
    fn writer_only_seed_does_not_expand_related() {
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let writer = Credit {
            id: Some(44),
            name: "Jonathan Aibel".into(),
            job: "Writer".into(),
        };
        let mut obs = observations_from_film(
            "Kung Fu Panda",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Animation".into(), "Family".into()],
            &[writer.clone()],
            &[],
            Some(2008),
            None,
        );
        obs.extend(observations_from_film(
            "Kung Fu Panda 2",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.3), 1, false),
            Some(0.3),
            &["Animation".into(), "Family".into()],
            &[writer.clone()],
            &[],
            Some(2011),
            None,
        ));
        let profile = build_profile(&obs);
        let seed = test_seed("Kung Fu Panda", 1, vec![writer]);
        let mut seed = seed;
        seed.genres = vec!["Animation".into(), "Family".into()];
        assert!(
            seed_expands_related(&seed, &profile),
            "animation seeds may expand; weak similar-to still stays off New"
        );
    }

    #[test]
    fn composer_only_seed_expands_bounded_recommendations_and_similar() {
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
        let p = rating_profile(&[4.0; 8]).unwrap();
        let composer = Credit {
            id: Some(12),
            name: "Michael Giacchino".into(),
            job: "Original Music Composer".into(),
        };
        let mut obs = observations_from_film(
            "The Batman",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into()],
            &[composer.clone()],
            &[],
            Some(2022),
            None,
        );
        obs.extend(observations_from_film(
            "Up",
            5.0,
            Some(2),
            &interaction_signal(5.0, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Animation".into(), "Family".into()],
            &[composer.clone()],
            &[],
            Some(2009),
            None,
        ));
        let profile = build_profile(&obs);
        let seed = test_seed("The Batman", 1, vec![composer]);
        assert!(
            seed_expands_related(&seed, &profile),
            "composer-led positive-like films still expand recommendations"
        );
        assert!(
            seed_allows_similar(&seed, &profile),
            "eligible seeds may use bounded similar-to retrieval"
        );
    }

    #[test]
    fn upsert_drops_non_movies() {
        let mut map = HashMap::new();
        upsert_candidate(
            &mut map,
            Candidate {
                tmdb_id: Some(1),
                title: "A Show".into(),
                year: Some(2020),
                poster: None,
                genres: vec![],
                credits: vec![],
                keywords: vec![],
                runtime: None,
                vote_count: None,
                watchlist: false,
                sources: vec![],
                friend_affinity: 0.0,
                tmdb_related: 0.0,
                media_kind: MediaKind::TvSeries,
            },
        );
        upsert_candidate(
            &mut map,
            Candidate {
                tmdb_id: Some(2),
                title: "A Movie".into(),
                year: Some(2020),
                poster: None,
                genres: vec![],
                credits: vec![],
                keywords: vec![],
                runtime: None,
                vote_count: None,
                watchlist: false,
                sources: vec![],
                friend_affinity: 0.0,
                tmdb_related: 0.0,
                media_kind: MediaKind::Movie,
            },
        );
        assert_eq!(map.len(), 1);
        assert_eq!(map.values().next().unwrap().title, "A Movie");
    }

    #[test]
    fn retrieve_membership_follows_current_seeds_not_a_union() {
        use crate::storage::db::Database;
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
        let db = Database::in_memory().unwrap();
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
            &interaction_signal(4.5, &p, Some(0.2), 1, false),
            Some(0.2),
            &["Science Fiction".into()],
            &[dp.clone()],
            &[],
            Some(2021),
            None,
        ));
        let profile = build_profile(&obs);
        let extra_from_dune = crate::models::LibraryItem::catalog(
            "tmdb:909".into(),
            "Only From Dune".into(),
            Some(2000),
            None,
            None,
            None,
        );
        let extra_from_batman = crate::models::LibraryItem::catalog(
            "tmdb:808".into(),
            "Only From Batman".into(),
            Some(2001),
            None,
            None,
            None,
        );
        let mut batman = test_seed("The Batman", 1, vec![dp.clone()]);
        batman.tmdb_id = Some(1);
        batman.recommendations = vec![extra_from_batman];
        let mut dune = test_seed("Dune", 1, vec![dp]);
        dune.tmdb_id = Some(2);
        dune.recommendations = vec![extra_from_dune];
        let seen = HashSet::new();
        let both = retrieve(&db, &[batman.clone(), dune.clone()], &profile, &seen, false).unwrap();
        assert!(both.iter().any(|c| c.tmdb_id == Some(909)));
        assert!(both.iter().any(|c| c.tmdb_id == Some(808)));
        let without_dune = retrieve(&db, &[batman], &profile, &seen, false).unwrap();
        assert!(
            without_dune.iter().all(|c| c.tmdb_id != Some(909)),
            "removed seed must not leave a candidate union, got {:?}",
            without_dune.iter().map(|c| &c.title).collect::<Vec<_>>()
        );
    }

    fn neighbor(id: i64, title: &str) -> LibraryItem {
        LibraryItem::catalog(
            format!("tmdb:{id}"),
            title.into(),
            Some(2000),
            None,
            None,
            None,
        )
    }

    #[test]
    fn upsert_merges_sources_from_every_seed() {
        let mut map = HashMap::new();
        let item = neighbor(50, "Shared");
        for seed_id in [1, 2, 3, 4, 5] {
            push_related(
                &mut map,
                &HashSet::new(),
                &{
                    let mut s = test_seed("Seed", 1, vec![]);
                    s.tmdb_id = Some(seed_id);
                    s
                },
                &item,
                RetrievalKind::RelatedRecommendations,
                format!("recommended from {seed_id}"),
            );
        }
        assert_eq!(map.len(), 1);
        let c = map.values().next().unwrap();
        assert_eq!(c.sources.len(), 5, "got {:?}", c.sources);
    }

    #[test]
    fn held_out_liked_neighbor_is_retrieved() {
        use crate::storage::db::Database;
        use crate::taste::features::build_profile;
        let db = Database::in_memory().unwrap();
        let profile = build_profile(&[]);
        let mut seed = test_seed("The Batman", 1, vec![]);
        seed.tmdb_id = Some(1);
        seed.recommendations = vec![neighbor(414_906, "Dune analog")];
        let mut seen = HashSet::new();
        seen.insert(identity_key(Some(1), "The Batman", Some(2011)));
        let out = retrieve(&db, &[seed], &profile, &seen, false).unwrap();
        assert!(
            out.iter().any(|c| c.tmdb_id == Some(414_906)),
            "a held-out liked neighbor of an eligible seed must re-enter the pool"
        );
        let mix = crate::taste::eval::source_mix(
            &out.iter()
                .map(|c| crate::taste::score::score_candidate(&profile, c))
                .collect::<Vec<_>>(),
        );
        assert!(mix.related_recommendations >= 1);
    }

    #[test]
    fn fifty_first_eligible_seed_still_expands() {
        use crate::storage::db::Database;
        use crate::taste::features::build_profile;
        use crate::taste::preference::{interaction_signal, rating_profile};
        let db = Database::in_memory().unwrap();
        let p = rating_profile(&[4.0; 8]).unwrap();
        let profile = build_profile(&[]);
        let mut films = Vec::new();
        for i in 0..51 {
            let mut seed = test_seed(&format!("Liked {i}"), 1, vec![]);
            seed.tmdb_id = Some(1000 + i);
            seed.rating = Some(4.5);
            seed.signal = Some(interaction_signal(4.5, &p, Some(0.4), 1, false));
            seed.recommendations = vec![neighbor(5000 + i, &format!("N{i}"))];
            films.push(seed);
        }
        let seen = HashSet::new();
        let out = retrieve(&db, &films, &profile, &seen, false).unwrap();
        assert!(
            out.iter().any(|c| c.tmdb_id == Some(5050)),
            "51st eligible seed must still contribute a neighbor"
        );
    }

    #[test]
    fn disliked_seed_does_not_expand() {
        use crate::storage::db::Database;
        use crate::taste::features::build_profile;
        use crate::taste::preference::{interaction_signal, rating_profile};
        let db = Database::in_memory().unwrap();
        let p = rating_profile(&[4.0; 8]).unwrap();
        let profile = build_profile(&[]);
        let mut seed = test_seed("Hated", 1, vec![]);
        seed.rating = Some(1.0);
        seed.signal = Some(interaction_signal(1.0, &p, Some(0.4), 1, false));
        seed.recommendations = vec![neighbor(77, "From hated")];
        let out = retrieve(&db, &[seed], &profile, &HashSet::new(), false).unwrap();
        assert!(
            out.iter().all(|c| c.tmdb_id != Some(77)),
            "disliked films must not seed neighbors"
        );
    }

    #[test]
    fn composer_seed_uses_recommendations_and_bounded_similar() {
        use crate::storage::db::Database;
        use crate::taste::features::{build_profile, observations_from_film};
        use crate::taste::preference::{interaction_signal, rating_profile};
        let db = Database::in_memory().unwrap();
        let p = rating_profile(&[4.0; 8]).unwrap();
        let composer = Credit {
            id: Some(12),
            name: "Michael Giacchino".into(),
            job: "Original Music Composer".into(),
        };
        let mut obs = observations_from_film(
            "The Batman",
            5.0,
            Some(1),
            &interaction_signal(5.0, &p, Some(0.5), 1, false),
            Some(0.5),
            &["Crime".into()],
            &[composer.clone()],
            &[],
            Some(2022),
            None,
        );
        obs.extend(observations_from_film(
            "Jurassic World",
            4.5,
            Some(2),
            &interaction_signal(4.5, &p, Some(0.4), 1, false),
            Some(0.4),
            &["Science Fiction".into()],
            &[composer.clone()],
            &[],
            Some(2015),
            None,
        ));
        let profile = build_profile(&obs);
        let mut seed = test_seed("The Batman", 1, vec![composer]);
        seed.recommendations = vec![neighbor(11, "From recs")];
        seed.similar = vec![neighbor(22, "From similar")];
        let out = retrieve(&db, &[seed], &profile, &HashSet::new(), false).unwrap();
        assert!(out.iter().any(|c| c.tmdb_id == Some(11)));
        assert!(out.iter().any(|c| c.tmdb_id == Some(22)));
    }

    #[test]
    fn related_recommendations_count_as_related_only() {
        use crate::taste::features::build_profile;
        use crate::taste::score::score_candidate;
        let profile = build_profile(&[]);
        let c = Candidate {
            tmdb_id: Some(1),
            title: "Rec".into(),
            year: Some(2020),
            poster: None,
            genres: vec!["Drama".into()],
            credits: vec![],
            keywords: vec![],
            runtime: Some(100),
            vote_count: Some(10),
            watchlist: false,
            sources: vec![RetrievalSource {
                kind: RetrievalKind::RelatedRecommendations,
                label: "recommended from X".into(),
                seed_tmdb_id: Some(9),
                seed_rating: None,
            }],
            friend_affinity: 0.0,
            tmdb_related: 1.0,
            media_kind: MediaKind::Movie,
        };
        let scored = score_candidate(&profile, &c);
        assert!(crate::taste::confidence::related_only(&scored));
    }

    #[test]
    fn fair_pool_keeps_a_neighbor_per_seed() {
        let mut map = HashMap::new();
        for i in 0..80i64 {
            let mut seed = test_seed("S", 1, vec![]);
            seed.tmdb_id = Some(i);
            push_related(
                &mut map,
                &HashSet::new(),
                &seed,
                &neighbor(10_000 + i, &format!("only-{i}")),
                RetrievalKind::RelatedRecommendations,
                format!("recommended from {i}"),
            );
            for extra in 0..20 {
                push_related(
                    &mut map,
                    &HashSet::new(),
                    &seed,
                    &neighbor(20_000 + i * 20 + extra, &format!("pad-{i}-{extra}")),
                    RetrievalKind::RelatedRecommendations,
                    format!("recommended from {i}"),
                );
            }
        }
        let selected = select_fair_pool(map, 100);
        let mut seeds_kept = HashSet::new();
        for c in &selected {
            for s in &c.sources {
                if let Some(id) = s.seed_tmdb_id {
                    seeds_kept.insert(id);
                }
            }
        }
        assert_eq!(
            seeds_kept.len(),
            80,
            "fair cap must keep at least one neighbor per seed, kept {}",
            seeds_kept.len()
        );
    }

    #[test]
    fn filmography_keeps_only_the_affinity_job() {
        use crate::taste::features::FeatureFamily;
        assert!(keep_person_credit("Director", FeatureFamily::Director));
        assert!(!keep_person_credit("Producer", FeatureFamily::Director));
        assert!(!keep_person_credit("Writer", FeatureFamily::Director));
        assert!(keep_person_credit(
            "Director of Photography",
            FeatureFamily::Cinematographer
        ));
        assert!(!keep_person_credit(
            "Director",
            FeatureFamily::Cinematographer
        ));
        assert!(!keep_person_credit("Actor", FeatureFamily::Cinematographer));
    }
}
