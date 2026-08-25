use crate::catalog::tmdb;
use crate::letterboxd::posters::poster_url;
use crate::letterboxd::rss::parse_activity_payload;
use crate::models::LibraryItem;
use crate::storage::db::Database;
use crate::taste::features::{Credit, FeatureFamily, FeatureProfile, Keyword};
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
    pub similar: Vec<LibraryItem>,
    pub runtime: Option<i32>,
    pub poster: Option<String>,
    pub vote_count: Option<i64>,
    pub signal: Option<InteractionSignal>,
    pub age_years: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RetrievalKind {
    Related,
    Filmography,
    Friend,
    Watchlist,
    Exploration,
    Discovery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalSource {
    pub kind: RetrievalKind,
    pub label: String,
    pub seed_tmdb_id: Option<i64>,
}

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
    let now = chrono::Utc::now();
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
              COALESCE(m.poster_path, smr.cached_poster_url),
              m.vote_count
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
            WHERE ums.current_rating IS NOT NULL
               OR ums.watched = 1
               OR ums.watchlist = 1
               OR smr.on_watchlist = 1
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let credits_json: Option<String> = row.get(11)?;
            let cast_json: Option<String> = row.get(12)?;
            let crew_json: Option<String> = row.get(13)?;
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
                credits: parse_credits(credits_json.as_deref(), cast_json.as_deref(), crew_json.as_deref()),
                keywords: parse_keywords(row.get::<_, Option<String>>(14)?),
                similar: parse_similar(row.get::<_, Option<String>>(15)?),
                runtime: row.get(16)?,
                poster: poster_url(row.get(17)?),
                vote_count: row.get(18)?,
                signal: None,
                age_years: None,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut films: Vec<FilmRecord> = rows.filter_map(|r| r.ok()).collect();
    let ratings: Vec<f32> = films.iter().filter_map(|f| f.rating).collect();
    let profile = rating_profile(&ratings);
    for film in &mut films {
        if let Some(date) = film.last_date.as_deref() {
            film.age_years = years_since(date, now);
        }
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
    Ok(films)
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
) -> Result<Vec<Candidate>, String> {
    let mut by_key: HashMap<String, Candidate> = HashMap::new();
    let mut used = HashSet::new();

    let mut seeds: Vec<&FilmRecord> = films
        .iter()
        .filter(|f| f.signal.is_some() && f.rating.is_some())
        .collect();
    seeds.sort_by(|a, b| {
        let sa = a.signal.as_ref().unwrap();
        let sb = b.signal.as_ref().unwrap();
        (sb.preference.absolute * sb.effective_weight)
            .partial_cmp(&(sa.preference.absolute * sa.effective_weight))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for seed in seeds.iter().take(40) {
        for item in seed.similar.iter().take(8) {
            let tmdb_id = tmdb_id_from_item(item);
            let key = identity_key(tmdb_id, &item.title, item.year);
            if seen.contains(&key) || !used.insert(key.clone()) {
                continue;
            }
            upsert_candidate(
                &mut by_key,
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
                    sources: vec![RetrievalSource {
                        kind: RetrievalKind::Related,
                        label: format!("similar to {}", seed.title),
                        seed_tmdb_id: seed.tmdb_id,
                    }],
                    friend_affinity: 0.0,
                    tmdb_related: 1.0,
                },
            );
        }
    }

    let people: Vec<_> = profile
        .affinities
        .iter()
        .filter(|a| {
            matches!(
                a.key.family,
                FeatureFamily::Director
                    | FeatureFamily::Writer
                    | FeatureFamily::Cinematographer
                    | FeatureFamily::Composer
            ) && a.key.id.is_some()
                && a.confidence > 0.4
                && a.weighted_mean > 0.15
        })
        .take(8)
        .collect();
    for person in people {
        let Some(pid) = person.key.id else { continue };
        let credits = tmdb::person_movie_credits(db, pid).unwrap_or_default();
        for credit in credits.into_iter().take(20) {
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
                        job: job_for_family(person.key.family),
                    }],
                    keywords: Vec::new(),
                    runtime: None,
                    vote_count: None,
                    watchlist: false,
                    sources: vec![RetrievalSource {
                        kind: RetrievalKind::Filmography,
                        label: person.key.name.clone(),
                        seed_tmdb_id: None,
                    }],
                    friend_affinity: 0.0,
                    tmdb_related: 0.0,
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
                }],
                friend_affinity: 0.0,
                tmdb_related: 0.0,
            },
        );
    }

    let friend_hits = friend_candidates(db, films, seen)?;
    for c in friend_hits {
        upsert_candidate(&mut by_key, c);
    }

    for aff in profile
        .affinities
        .iter()
        .filter(|a| a.confidence > 0.5 && a.appearances <= 4 && a.weighted_mean > 0.25)
        .take(6)
    {
        if aff.key.family != FeatureFamily::Keyword && aff.key.family != FeatureFamily::Genre {
            continue;
        }
        let _ = aff;
        // Exploration is represented by keeping sparse-affinity filmography/related
        // candidates; no extra network hop.
    }

    let mut out: Vec<Candidate> = by_key.into_values().collect();
    hydrate_local_metadata(db, &mut out)?;
    if out.len() > 1000 {
        out.truncate(1000);
    }
    Ok(out)
}

fn job_for_family(family: FeatureFamily) -> String {
    match family {
        FeatureFamily::Director => "Director".into(),
        FeatureFamily::Writer => "Writer".into(),
        FeatureFamily::Cinematographer => "Director of Photography".into(),
        FeatureFamily::Composer => "Original Music Composer".into(),
        FeatureFamily::Actor => "Actor".into(),
        _ => "Crew".into(),
    }
}

fn upsert_candidate(map: &mut HashMap<String, Candidate>, incoming: Candidate) {
    let key = identity_key(incoming.tmdb_id, &incoming.title, incoming.year);
    map.entry(key)
        .and_modify(|existing| {
            existing.sources.extend(incoming.sources.clone());
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
                label: format!(
                    "loved by {} friend(s)",
                    contribs.len()
                ),
                seed_tmdb_id: None,
            }],
            friend_affinity,
            tmdb_related: 0.0,
        });
    }
    Ok(out)
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
        if !c.genres.is_empty() && !c.credits.is_empty() {
            continue;
        }
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
        if let Ok((genres, credits, cast, crew, keywords, runtime, vote_count, poster, title, year)) =
            row
        {
            if c.genres.is_empty() {
                c.genres = json_vec(genres);
            }
            if c.credits.is_empty() {
                c.credits = parse_credits(credits.as_deref(), cast.as_deref(), crew.as_deref());
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

pub fn enrich_missing(db: &Database, candidates: &mut [Candidate], cap: usize) -> usize {
    let mut n = 0;
    for c in candidates.iter_mut() {
        if n >= cap {
            break;
        }
        let Some(tid) = c.tmdb_id else { continue };
        if !c.credits.is_empty() && !c.genres.is_empty() {
            continue;
        }
        if tmdb::refresh_movie_catalog(db, tid, false).is_ok() {
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

fn parse_similar(raw: Option<String>) -> Vec<LibraryItem> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    if let Ok(items) = serde_json::from_str::<Vec<LibraryItem>>(&raw) {
        if items.iter().any(|item| !item.title.is_empty()) {
            return items;
        }
    }
    serde_json::from_str::<Vec<serde_json::Value>>(&raw)
        .ok()
        .map(|vals| vals.iter().filter_map(tmdb::library_item_from_tmdb_value).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friend_overlap_gate() {
        assert_eq!(friend_similarity(&[(5.0, 5.0), (4.0, 4.0)], &[5.0, 4.0]), 0.0);
        let pairs: Vec<(f32, f32)> = (0..20)
            .map(|i| (3.0 + (i % 3) as f32 * 0.5, 3.0 + (i % 3) as f32 * 0.5))
            .collect();
        assert!(friend_similarity(&pairs, &[]).abs() > 0.3);
    }

    #[test]
    fn identity_prefers_tmdb() {
        assert_eq!(identity_key(Some(99), "Heat", Some(1995)), "tmdb:99");
        assert_eq!(identity_key(None, "Heat", Some(1995)), "heat|1995");
    }
}
