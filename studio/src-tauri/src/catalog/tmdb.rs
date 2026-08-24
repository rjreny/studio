use crate::letterboxd::normalize::{normalize_title, parse_year};
use crate::letterboxd::posters::{backfill_letterboxd_posters, cache_poster_for_siblings, full_poster_url};
use crate::storage::db::Database;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use uuid::Uuid;

const TMDB_BASE: &str = "https://api.themoviedb.org/3";

pub fn set_api_key(key: &str) -> Result<(), String> {
    keyring::Entry::new("studio", "tmdb_api_key")
        .map_err(|e| e.to_string())?
        .set_password(key)
        .map_err(|e| e.to_string())
}

pub fn get_api_key() -> Result<Option<String>, String> {
    keyring::Entry::new("studio", "tmdb_api_key")
        .map_err(|e| e.to_string())?
        .get_password()
        .map(|k| {
            let trimmed = k.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .or_else(|e| {
            if e.to_string().contains("No matching entry") {
                Ok(None)
            } else {
                Err(e.to_string())
            }
        })
}

pub fn clear_api_key() -> Result<(), String> {
    keyring::Entry::new("studio", "tmdb_api_key")
        .map_err(|e| e.to_string())?
        .set_password("")
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct TmdbSearchResponse {
    results: Vec<TmdbSearchResult>,
}

#[derive(Deserialize)]
struct TmdbSearchResult {
    id: i64,
    title: String,
    release_date: Option<String>,
}

pub fn enrich_catalog(db: &Database) -> Result<u32, String> {
    let mut total = 0u32;
    let has_key = get_api_key()?.is_some();
    if let Some(api_key) = get_api_key()? {
        for _ in 0..40 {
            let batch = enrich_unmatched_batch(db, &api_key, 50)?;
            total += batch;
            if batch == 0 {
                break;
            }
        }
    }
    // Only hit Letterboxd oEmbed when there is no TMDB key configured.
    if !has_key {
        for _ in 0..8 {
            let batch = backfill_letterboxd_posters(db, 40)?;
            total += batch;
            if batch == 0 {
                break;
            }
        }
    }
    Ok(total)
}

pub fn enrich_unmatched(db: &Database) -> Result<u32, String> {
    enrich_catalog(db)
}

fn enrich_unmatched_batch(db: &Database, api_key: &str, limit: u32) -> Result<u32, String> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT smr.id, smr.raw_identity, smr.normalized_title, smr.release_year
             FROM source_movie_records smr
             JOIN movie_links ml ON ml.source_movie_record_id = smr.id
             WHERE ml.match_state IN ('unmatched', 'ambiguous')
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String, Option<i32>)> = stmt
        .query_map(params![limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut enriched = 0u32;
    for (smr_id, raw, normalized, year) in rows {
        if try_tmdb_id_from_raw(db, &smr_id, &raw)? {
            enriched += 1;
            continue;
        }
        if try_exact_match(db, &smr_id, &normalized, year)? {
            enriched += 1;
            continue;
        }
        if search_and_queue(db, api_key, &smr_id, &raw, &normalized, year)? > 0 {
            enriched += 1;
        }
    }
    Ok(enriched)
}

fn try_tmdb_id_from_raw(db: &Database, smr_id: &str, raw: &str) -> Result<bool, String> {
    let Some(id) = serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("tmdb_id").and_then(|x| x.as_i64()))
        .filter(|&id| id > 0)
    else {
        return Ok(false);
    };
    let movie_id = upsert_tmdb_movie(db, id)?;
    confirm_link(db, smr_id, &movie_id, "export_tmdb_id")?;
    Ok(true)
}

fn confirm_link(db: &Database, smr_id: &str, movie_id: &str, method: &str) -> Result<(), String> {
    db.conn()
        .execute(
            "UPDATE movie_links SET movie_id = ?2, match_state = 'confirmed',
             match_method = ?3, confidence = 1.0, confirmed_at = datetime('now')
             WHERE source_movie_record_id = ?1",
            params![smr_id, movie_id, method],
        )
        .map_err(|e| e.to_string())?;
    propagate_movie_link(db, smr_id, movie_id)?;
    Ok(())
}

fn propagate_movie_link(db: &Database, smr_id: &str, movie_id: &str) -> Result<(), String> {
    let (normalized, year): (String, Option<i32>) = db
        .conn()
        .query_row(
            "SELECT normalized_title, release_year FROM source_movie_records WHERE id = ?1",
            params![smr_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    db.conn()
        .execute(
            "UPDATE movie_links SET movie_id = ?3, match_state = 'confirmed',
             match_method = 'propagated', confidence = 1.0, confirmed_at = datetime('now')
             WHERE source_movie_record_id IN (
               SELECT id FROM source_movie_records
               WHERE normalized_title = ?1 AND release_year IS ?2
             ) AND match_state != 'confirmed'",
            params![normalized, year, movie_id],
        )
        .map_err(|e| e.to_string())?;

    let poster_path: Option<String> = db
        .conn()
        .query_row(
            "SELECT poster_path FROM movies WHERE id = ?1",
            params![movie_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some(path) = poster_path.filter(|p| !p.is_empty()) {
        cache_poster_for_siblings(db.conn(), smr_id, &full_poster_url(&path))?;
    }
    Ok(())
}

fn try_exact_match(
    db: &Database,
    smr_id: &str,
    normalized: &str,
    year: Option<i32>,
) -> Result<bool, String> {
    let movie_id: Option<String> = db
        .conn()
        .query_row(
            "SELECT movie_id FROM movie_aliases WHERE normalized_title = ?1 AND release_year IS ?2 LIMIT 1",
            params![normalized, year],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some(mid) = movie_id {
        confirm_link(db, smr_id, &mid, "exact_title_year")?;
        return Ok(true);
    }
    Ok(false)
}

fn search_and_queue(
    db: &Database,
    api_key: &str,
    smr_id: &str,
    raw: &str,
    normalized: &str,
    year: Option<i32>,
) -> Result<u32, String> {
    let title = parse_raw_title(raw);
    let mut url = format!(
        "{TMDB_BASE}/search/movie?api_key={api_key}&query={}",
        urlencoding_simple(&title)
    );
    if let Some(y) = year {
        url.push_str(&format!("&primary_release_year={y}"));
    }
    let response = ureq::get(&url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let parsed: TmdbSearchResponse = serde_json::from_str(&response).map_err(|e| e.to_string())?;

    if let Some(result) = pick_search_match(&parsed.results, normalized, year) {
        let movie_id = upsert_tmdb_movie(db, result.id)?;
        confirm_link(db, smr_id, &movie_id, "tmdb_search")?;
        return Ok(1);
    }

    let state = if parsed.results.is_empty() {
        "unmatched"
    } else {
        "ambiguous"
    };
    db.conn()
        .execute(
            "UPDATE movie_links SET match_state = ?2, match_method = 'tmdb_search',
             confidence = NULL, confirmed_at = NULL WHERE source_movie_record_id = ?1",
            params![smr_id, state],
        )
        .map_err(|e| e.to_string())?;
    Ok(0)
}

fn pick_search_match<'a>(
    results: &'a [TmdbSearchResult],
    normalized: &str,
    year: Option<i32>,
) -> Option<&'a TmdbSearchResult> {
    let exact_year: Vec<_> = results
        .iter()
        .filter(|r| titles_match(&r.title, normalized) && release_year(r) == year)
        .collect();
    if exact_year.len() == 1 {
        return Some(exact_year[0]);
    }

    let title_only: Vec<_> = results
        .iter()
        .filter(|r| titles_match(&r.title, normalized))
        .collect();
    if title_only.len() == 1 {
        return Some(title_only[0]);
    }

    if let Some(y) = year {
        let near: Vec<_> = results
            .iter()
            .filter(|r| {
                titles_match(&r.title, normalized)
                    && release_year(r)
                        .map(|ry| (ry - y).abs() <= 1)
                        .unwrap_or(false)
            })
            .collect();
        if near.len() == 1 {
            return Some(near[0]);
        }
    }

    if results.len() == 1 {
        return Some(&results[0]);
    }

    None
}

fn titles_match(candidate: &str, normalized: &str) -> bool {
    normalize_title(candidate) == normalized
}

fn release_year(result: &TmdbSearchResult) -> Option<i32> {
    result
        .release_date
        .as_deref()
        .and_then(|d| d.get(0..4))
        .and_then(parse_year)
}

fn upsert_tmdb_movie(db: &Database, tmdb_id: i64) -> Result<String, String> {
    if let Some(existing) = db
        .conn()
        .query_row(
            "SELECT id, COALESCE(poster_path, '') FROM movies WHERE tmdb_id = ?1",
            params![tmdb_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        if !existing.1.is_empty() {
            return Ok(existing.0);
        }
    }
    let api_key = get_api_key()?.ok_or("missing api key")?;
    let url = format!(
        "{TMDB_BASE}/movie/{tmdb_id}?api_key={api_key}&append_to_response=credits,reviews,recommendations,similar"
    );
    let body = ureq::get(&url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let title = v["title"].as_str().unwrap_or("Unknown").to_string();
    let year = v["release_date"]
        .as_str()
        .and_then(|d| d.get(0..4))
        .and_then(parse_year);
    let poster = v["poster_path"].as_str().unwrap_or("").to_string();

    if let Some((existing_id, _)) = db
        .conn()
        .query_row(
            "SELECT id, COALESCE(poster_path, '') FROM movies WHERE tmdb_id = ?1",
            params![tmdb_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        db.conn()
            .execute(
                "UPDATE movies SET canonical_title = ?2, release_year = ?3, poster_path = ?4,
                 backdrop_path = ?5, overview = ?6, runtime = ?7, vote_average = ?8, vote_count = ?9,
                 genres_json = ?10, cast_json = ?11, crew_json = ?12, similar_json = ?13,
                 reviews_json = ?14, enriched_at = datetime('now')
                 WHERE id = ?1",
                params![
                    existing_id,
                    title,
                    year,
                    if poster.is_empty() { None::<String> } else { Some(poster.clone()) },
                    v["backdrop_path"].as_str(),
                    v["overview"].as_str(),
                    v["runtime"].as_i64(),
                    v["vote_average"].as_f64(),
                    v["vote_count"].as_i64(),
                    serde_json::to_string(&genre_names(&v)).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&cast_names(&v)).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&crew_names(&v)).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&similar_titles(&v)).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&review_authors(&v)).unwrap_or_else(|_| "[]".into()),
                ],
            )
            .map_err(|e| e.to_string())?;
        return Ok(existing_id);
    }

    let id = Uuid::new_v4().to_string();
    db.conn()
        .execute(
            "INSERT INTO movies(id, canonical_title, release_year, tmdb_id, poster_path, backdrop_path,
             overview, runtime, vote_average, vote_count, genres_json, cast_json, crew_json, similar_json, reviews_json, enriched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, datetime('now'))",
            params![
                id,
                title,
                year,
                tmdb_id,
                if poster.is_empty() { None::<String> } else { Some(poster) },
                v["backdrop_path"].as_str(),
                v["overview"].as_str(),
                v["runtime"].as_i64(),
                v["vote_average"].as_f64(),
                v["vote_count"].as_i64(),
                serde_json::to_string(&genre_names(&v)).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&cast_names(&v)).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&crew_names(&v)).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&similar_titles(&v)).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&review_authors(&v)).unwrap_or_else(|_| "[]".into()),
            ],
        )
        .map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "INSERT OR IGNORE INTO movie_aliases(movie_id, normalized_title, release_year)
             VALUES (?1, ?2, ?3)",
            params![id, normalize_title(&title), year],
        )
        .map_err(|e| e.to_string())?;
    Ok(id)
}

fn parse_raw_title(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(title) = v.get("title").and_then(|t| t.as_str()) {
            return title.to_string();
        }
    }
    raw.to_string()
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

fn genre_names(v: &serde_json::Value) -> Vec<String> {
    v["genres"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|g| g["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn cast_names(v: &serde_json::Value) -> Vec<String> {
    v["credits"]["cast"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .take(12)
                .filter_map(|g| g["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn crew_names(v: &serde_json::Value) -> Vec<String> {
    v["credits"]["crew"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|g| {
                    let job = g["job"].as_str()?;
                    if [
                        "Director",
                        "Writer",
                        "Screenplay",
                        "Director of Photography",
                        "Production Design",
                    ]
                    .contains(&job)
                    {
                        Some(format!("{} ({})", g["name"].as_str()?, job))
                    } else {
                        None
                    }
                })
                .take(8)
                .collect()
        })
        .unwrap_or_default()
}

fn similar_titles(v: &serde_json::Value) -> Vec<serde_json::Value> {
    v["similar"]["results"]
        .as_array()
        .map(|arr| arr.iter().take(8).cloned().collect())
        .unwrap_or_default()
}

fn review_authors(v: &serde_json::Value) -> Vec<String> {
    v["reviews"]["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .take(5)
                .filter_map(|r| {
                    let author = r["author"].as_str()?;
                    let content = r["content"].as_str()?.chars().take(120).collect::<String>();
                    Some(format!("{author}: {content}…"))
                })
                .collect()
        })
        .unwrap_or_default()
}
