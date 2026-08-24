use crate::storage::db::Database;
use rusqlite::{params, Transaction};

#[derive(Default)]
pub struct SourceMovieMeta {
    pub poster: Option<String>,
    pub tmdb_id: Option<i64>,
}

pub fn poster_from_rss_body(body: &str) -> Option<String> {
    let lower = body.to_lowercase();
    let mut search_from = 0;
    while let Some(img_start) = lower[search_from..].find("<img") {
        let idx = search_from + img_start;
        let tag_end = lower[idx..].find('>')? + idx;
        let tag = &body[idx..=tag_end];
        if let Some(src) = extract_attr(tag, "src") {
            if src.contains("ltrbxd.com") || src.starts_with("http") {
                return Some(src);
            }
        }
        search_from = tag_end + 1;
    }
    None
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=\"");
    let lower = tag.to_lowercase();
    let pattern_lower = pattern.to_lowercase();
    let start = lower.find(&pattern_lower)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub fn full_poster_url(path: &str) -> String {
    if path.starts_with("http") {
        path.to_string()
    } else {
        format!("https://image.tmdb.org/t/p/w500{path}")
    }
}

pub fn letterboxd_oembed_poster(uri: &str) -> Result<Option<String>, String> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let page = if trimmed.starts_with("http") {
        trimmed.to_string()
    } else {
        format!("https://letterboxd.com{trimmed}")
    };
    let url = format!(
        "https://letterboxd.com/oembed?url={}",
        urlencoding_simple(&page)
    );
    let body = ureq::get(&url)
        .set("User-Agent", "Studio/0.1 (local film app)")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(v["thumbnail_url"].as_str().map(String::from))
}

/// Letterboxd oEmbed is a last resort when no TMDB key is configured.
/// Posters are cached locally and failed lookups are not retried.
pub fn backfill_letterboxd_posters(db: &Database, limit: u32) -> Result<u32, String> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT id, source_record_key, external_id
             FROM source_movie_records
             WHERE external_id IS NOT NULL AND TRIM(external_id) != ''
             AND (cached_poster_url IS NULL OR TRIM(cached_poster_url) = '')
             AND poster_fetch_failed = 0
             LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut updated = 0u32;
    for (smr_id, _key, uri) in rows {
        match letterboxd_oembed_poster(&uri) {
            Ok(Some(poster)) => {
                cache_poster_for_smr(db.conn(), &smr_id, &poster)?;
                updated += 1;
            }
            Ok(None) => {
                mark_poster_fetch_failed(db.conn(), &smr_id)?;
            }
            Err(_) => {
                mark_poster_fetch_failed(db.conn(), &smr_id)?;
            }
        }
    }
    Ok(updated)
}

pub fn cache_poster_for_smr(
    conn: &rusqlite::Connection,
    smr_id: &str,
    poster_url: &str,
) -> Result<(), String> {
    if poster_url.trim().is_empty() {
        return Ok(());
    }
    conn.execute(
        "UPDATE source_movie_records
         SET cached_poster_url = ?2,
             raw_identity = json_set(COALESCE(NULLIF(raw_identity, ''), '{}'), '$.poster', json_quote(?2))
         WHERE id = ?1",
        params![smr_id, poster_url.trim()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cache_poster_for_siblings(
    conn: &rusqlite::Connection,
    smr_id: &str,
    poster_url: &str,
) -> Result<(), String> {
    cache_poster_for_smr(conn, smr_id, poster_url)?;
    let (normalized, year): (String, Option<i32>) = conn
        .query_row(
            "SELECT normalized_title, release_year FROM source_movie_records WHERE id = ?1",
            params![smr_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE source_movie_records
         SET cached_poster_url = ?3,
             raw_identity = json_set(COALESCE(NULLIF(raw_identity, ''), '{}'), '$.poster', json_quote(?3))
         WHERE normalized_title = ?1 AND release_year IS ?2
         AND (cached_poster_url IS NULL OR TRIM(cached_poster_url) = '')",
        params![normalized, year, poster_url.trim()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn mark_poster_fetch_failed(conn: &rusqlite::Connection, smr_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE source_movie_records SET poster_fetch_failed = 1 WHERE id = ?1",
        params![smr_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn merge_source_movie_metadata(
    conn: &rusqlite::Connection,
    source_record_key: &str,
    poster: Option<&str>,
    tmdb_id: Option<i64>,
) -> Result<(), String> {
    if poster.is_none() && tmdb_id.is_none() {
        return Ok(());
    }

    if let Some(url) = poster.filter(|p| !p.trim().is_empty()) {
        conn.execute(
            "UPDATE source_movie_records
             SET cached_poster_url = ?2,
                 raw_identity = json_set(
                   COALESCE(NULLIF(raw_identity, ''), '{}'),
                   '$.poster', json_quote(?2)
                 )
             WHERE source_record_key = ?1
             AND (cached_poster_url IS NULL OR TRIM(cached_poster_url) = '')",
            params![source_record_key, url],
        )
        .map_err(|e| e.to_string())?;
    }

    if let Some(id) = tmdb_id {
        conn.execute(
            "UPDATE source_movie_records
             SET raw_identity = json_set(
               COALESCE(NULLIF(raw_identity, ''), '{}'),
               '$.tmdb_id', json(?2)
             )
             WHERE source_record_key = ?1",
            params![source_record_key, id],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn merge_source_movie_metadata_tx(
    tx: &Transaction<'_>,
    source_record_key: &str,
    poster: Option<&str>,
    tmdb_id: Option<i64>,
) -> Result<(), String> {
    merge_source_movie_metadata(tx, source_record_key, poster, tmdb_id)
}

pub fn parse_tmdb_id(raw: &str) -> Option<i64> {
    if raw.trim().is_empty() {
        return None;
    }
    raw.trim().parse().ok().filter(|&id| id > 0)
}

pub fn tmdb_id_from_rss_body(body: &str) -> Option<i64> {
    for name in ["filmId", "movieId", "tmdbId", "tmdbMovieId"] {
        if let Some(raw) = rss_tag(body, name) {
            if let Some(id) = parse_tmdb_id(&raw) {
                return Some(id);
            }
        }
    }
    None
}

fn rss_tag(xml: &str, name: &str) -> Option<String> {
    for marker in [name, &format!("letterboxd:{name}")] {
        let open = format!("<{marker}");
        let lower = xml.to_lowercase();
        let start = lower.find(&open.to_lowercase())?;
        let after = &xml[start..];
        let content_start = after.find('>')? + 1;
        let rest = &after[content_start..];
        let close = format!("</{marker}>");
        let end = rest.to_lowercase().find(&close.to_lowercase())?;
        return Some(rest[..end].trim().to_string());
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::letterboxd::import::upsert_source_movie;
    use crate::storage::db::Database;

    #[test]
    fn extracts_poster_from_rss_description() {
        let body = r#"<description><![CDATA[<p><img src="https://a.ltrbxd.com/resized/film-poster/240/240/70/68/abc.jpg" alt="Ant-Man"/></p>]]></description>"#;
        assert_eq!(
            poster_from_rss_body(body),
            Some("https://a.ltrbxd.com/resized/film-poster/240/240/70/68/abc.jpg".into())
        );
    }

    #[test]
    fn caches_http_poster_url_on_source_record() {
        let mut db = Database::in_memory().expect("db");
        let tx = db.transaction().expect("tx");
        let smr_id = upsert_source_movie(
            &tx,
            "letterboxd_export",
            "export|inception",
            "Inception",
            Some(2010),
            "https://letterboxd.com/film/inception/",
            &SourceMovieMeta::default(),
        )
        .expect("smr");
        tx.commit().expect("commit");

        let poster = "https://image.tmdb.org/t/p/w500/9gk7adHYeDvHkCSEqAvQNLV5Uge.jpg";
        cache_poster_for_smr(db.conn(), &smr_id, poster).expect("cache poster URL");

        let cached: String = db
            .conn()
            .query_row(
                "SELECT cached_poster_url FROM source_movie_records WHERE id = ?1",
                rusqlite::params![smr_id],
                |row| row.get(0),
            )
            .expect("cached url");
        assert_eq!(cached, poster);

        let identity: String = db
            .conn()
            .query_row(
                "SELECT json_extract(raw_identity, '$.poster') FROM source_movie_records WHERE id = ?1",
                rusqlite::params![smr_id],
                |row| row.get(0),
            )
            .expect("identity poster");
        assert_eq!(identity, poster);
    }
}
