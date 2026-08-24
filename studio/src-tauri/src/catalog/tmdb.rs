use crate::letterboxd::normalize::{normalize_title, parse_year};
use crate::letterboxd::posters::{backfill_letterboxd_posters, cache_poster_for_siblings, full_poster_url};
use crate::models::{EnrichReport, JobProgress, TmdbKeyStatus};
use crate::storage::db::Database;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use uuid::Uuid;

const TMDB_BASE: &str = "https://api.themoviedb.org/3";

pub fn set_api_key(key: &str) -> Result<(), String> {
    keyring::Entry::new("studio", "tmdb_api_key")
        .map_err(|e| e.to_string())?
        .set_password(key)
        .map_err(|e| e.to_string())?;
    match get_api_key()? {
        Some(stored) if stored == key => Ok(()),
        Some(_) => Err("Windows Credential Manager stored a different TMDB key than the one just saved".into()),
        None => Err(
            "Windows Credential Manager did not keep the TMDB key. Studio cannot match ZIP films without it."
                .into(),
        ),
    }
}

pub fn store_api_key(key: &str) -> Result<TmdbKeyStatus, String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("Paste a TMDB API key first".into());
    }
    let status = probe_key(trimmed)?;
    if status.valid != Some(true) {
        let previous = get_api_key()?.is_some();
        let mut rejected = status;
        rejected.stored = previous;
        if previous {
            rejected.last_error = Some(format!(
                "{}. Previous key is still stored.",
                rejected
                    .last_error
                    .unwrap_or_else(|| "TMDB rejected this key".into())
            ));
        }
        return Ok(rejected);
    }
    set_api_key(trimmed)?;
    Ok(TmdbKeyStatus {
        stored: true,
        valid: Some(true),
        kind: status.kind,
        last_error: None,
    })
}

pub fn key_status() -> Result<TmdbKeyStatus, String> {
    match get_api_key()? {
        Some(key) => {
            let mut status = probe_key(&key)?;
            status.stored = true;
            Ok(status)
        }
        None => Ok(TmdbKeyStatus {
            stored: false,
            valid: None,
            kind: None,
            last_error: None,
        }),
    }
}

fn probe_key(key: &str) -> Result<TmdbKeyStatus, String> {
    let kind = if is_bearer_token(key) {
        "accessToken"
    } else {
        "apiKey"
    };
    match tmdb_get(key, "/configuration") {
        Ok(_) => Ok(TmdbKeyStatus {
            stored: false,
            valid: Some(true),
            kind: Some(kind.into()),
            last_error: None,
        }),
        Err(err) => {
            let invalid = err.contains("401") || err.contains("403") || err.contains("Unauthorized");
            Ok(TmdbKeyStatus {
                stored: false,
                valid: if invalid { Some(false) } else { None },
                kind: Some(kind.into()),
                last_error: Some(if invalid {
                    "TMDB rejected this key. Use the API Key (v3) from themoviedb.org/settings/api.".into()
                } else {
                    format!("Could not reach TMDB: {err}")
                }),
            })
        }
    }
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

fn is_bearer_token(key: &str) -> bool {
    let k = key.trim();
    k.starts_with("eyJ") || (k.len() > 48 && k.bytes().filter(|b| *b == b'.').count() >= 2)
}

fn tmdb_get(key: &str, path_and_query: &str) -> Result<String, String> {
    let key = key.trim();
    let url = if is_bearer_token(key) {
        format!("{TMDB_BASE}{path_and_query}")
    } else {
        let sep = if path_and_query.contains('?') { '&' } else { '?' };
        format!("{TMDB_BASE}{path_and_query}{sep}api_key={key}")
    };
    let mut req = ureq::get(&url)
        .set("User-Agent", "Studio/0.1 (local film app)")
        .timeout(std::time::Duration::from_secs(8));
    if is_bearer_token(key) {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    req.call()
        .map_err(|e| e.to_string())?
        .into_string()
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

pub fn enrich_catalog(db: &Database) -> Result<EnrichReport, String> {
    enrich_catalog_with(db, &mut |_| {})
}

pub fn enrich_catalog_with(
    db: &Database,
    progress: &mut dyn FnMut(JobProgress),
) -> Result<EnrichReport, String> {
    let mut report = EnrichReport {
        has_key: false,
        key_valid: None,
        attempted: 0,
        matched: 0,
        posters: 0,
        remaining_unmatched: 0,
        remaining_without_poster: 0,
        errors: 0,
        last_error: None,
        log_path: None,
    };

    if let Some(api_key) = get_api_key()? {
        report.has_key = true;
        match tmdb_get(&api_key, "/configuration") {
            Ok(_) => report.key_valid = Some(true),
            Err(err) => {
                report.key_valid = Some(false);
                report.last_error = Some(err);
            }
        }
        if report.key_valid == Some(true) {
            let queue = list_unmatched(db)?;
            let total = queue.len() as u32;
            for (smr_id, raw, normalized, year) in queue {
                report.attempted += 1;
                progress(JobProgress {
                    job: "enrich".into(),
                    label: match_progress_label(report.attempted, total.max(1)),
                    current: report.attempted,
                    total: total.max(1),
                    posters: report.posters,
                    errors: report.errors,
                    done: false,
                    ..Default::default()
                });
                match enrich_one_film(db, &api_key, &smr_id, &raw, &normalized, year) {
                    Ok(true) => {
                        report.matched += 1;
                        report.posters += 1;
                    }
                    Ok(false) => {}
                    Err(err) => {
                        report.errors += 1;
                        report.last_error = Some(err);
                    }
                }
            }
        }
    }

    let _ = db.conn().execute(
        "UPDATE source_movie_records SET poster_fetch_failed = 0
         WHERE cached_poster_url IS NULL OR TRIM(cached_poster_url) = ''",
        [],
    );

    for _ in 0..12 {
        let before = report.posters;
        match backfill_letterboxd_posters(db, 40) {
            Ok(n) => {
                report.posters += n;
                progress(JobProgress {
                    job: "enrich".into(),
                    label: format!("Letterboxd posters · {} fetched", report.posters),
                    current: report.posters,
                    total: report.posters.max(1),
                    posters: report.posters,
                    errors: report.errors,
                    done: false,
                    ..Default::default()
                });
                if n == 0 || report.posters == before {
                    break;
                }
            }
            Err(err) => {
                report.errors += 1;
                report.last_error = Some(err);
                break;
            }
        }
    }

    report.remaining_unmatched = count_unmatched(db)?;
    report.remaining_without_poster = count_without_poster(db)?;
    progress(JobProgress {
        job: "enrich".into(),
        label: "Catalog pass complete".into(),
        current: report.attempted,
        total: report.attempted.max(1),
        posters: report.posters,
        errors: report.errors,
        done: false,
        ..Default::default()
    });
    Ok(report)
}

pub fn enrich_unmatched(db: &Database) -> Result<EnrichReport, String> {
    enrich_catalog(db)
}

fn count_unmatched(db: &Database) -> Result<u32, String> {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM movie_links WHERE match_state IN ('unmatched', 'ambiguous')",
            [],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|e| e.to_string())
}

fn count_without_poster(db: &Database) -> Result<u32, String> {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM source_movie_records smr
             LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
             LEFT JOIN movies m ON m.id = ml.movie_id
             WHERE (smr.cached_poster_url IS NULL OR TRIM(smr.cached_poster_url) = '')
               AND (m.poster_path IS NULL OR TRIM(m.poster_path) = '')",
            [],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|e| e.to_string())
}

pub(crate) fn match_progress_label(current: u32, total: u32) -> String {
    format!("Matching TMDB · {current}/{total}")
}

fn list_unmatched(
    db: &Database,
) -> Result<Vec<(String, String, String, Option<i32>)>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT smr.id, smr.raw_identity, smr.normalized_title, smr.release_year
             FROM source_movie_records smr
             JOIN movie_links ml ON ml.source_movie_record_id = smr.id
             WHERE ml.match_state IN ('unmatched', 'ambiguous')",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn enrich_one_film(
    db: &Database,
    api_key: &str,
    smr_id: &str,
    raw: &str,
    normalized: &str,
    year: Option<i32>,
) -> Result<bool, String> {
    if try_tmdb_id_from_raw(db, smr_id, raw)? {
        return Ok(true);
    }
    if try_exact_match(db, smr_id, normalized, year)? {
        return Ok(true);
    }
    Ok(search_and_queue(db, api_key, smr_id, raw, normalized, year)? > 0)
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

fn search_movies(
    api_key: &str,
    title: &str,
    year: Option<i32>,
) -> Result<TmdbSearchResponse, String> {
    let mut path = format!("/search/movie?query={}", percent_encode(title));
    if let Some(y) = year {
        path.push_str(&format!("&primary_release_year={y}"));
    }
    let response = tmdb_get(api_key, &path)?;
    serde_json::from_str(&response).map_err(|e| e.to_string())
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
    let mut parsed = search_movies(api_key, &title, year)?;
    if pick_search_match(&parsed.results, normalized, year).is_none() && year.is_some() {
        parsed = search_movies(api_key, &title, None)?;
    }

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
    let path = format!(
        "/movie/{tmdb_id}?append_to_response=credits,reviews,recommendations,similar"
    );
    let body = tmdb_get(&api_key, &path)?;
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

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treats_jwt_read_token_as_bearer() {
        assert!(is_bearer_token(
            "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJ0ZXN0In0.signature"
        ));
        assert!(!is_bearer_token("a1b2c3d4e5f678901234567890123456"));
    }

    #[test]
    fn percent_encodes_utf8_bytes() {
        assert_eq!(percent_encode("Amélie"), "Am%C3%A9lie");
        assert_eq!(percent_encode("The Matrix"), "The%20Matrix");
    }

    #[test]
    fn tmdb_keyring_persists_beyond_the_entry() {
        use keyring::credential::{CredentialBuilderApi as _, CredentialPersistence};
        let persistence = keyring::default::default_credential_builder().persistence();
        assert!(
            !matches!(persistence, CredentialPersistence::EntryOnly),
            "keyring is using the in-memory mock store; enable windows-native so the TMDB key survives across Entry::new calls"
        );
    }

    #[test]
    fn match_progress_uses_library_total_not_batch_size() {
        assert_eq!(match_progress_label(2, 1097), "Matching TMDB · 2/1097");
        assert_ne!(match_progress_label(2, 1097), "Matching TMDB · 2/50");
    }
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
