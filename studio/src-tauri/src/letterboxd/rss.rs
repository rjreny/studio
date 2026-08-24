use super::fingerprint::{row_fingerprint, source_record_key};
use super::import::{count_events, upsert_source_movie};
use super::normalize::{normalize_title, parse_year};
use super::posters::{poster_from_rss_body, tmdb_id_from_rss_body, SourceMovieMeta};
use crate::models::SyncResult;
use crate::storage::db::Database;
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use uuid::Uuid;

pub fn rss_url(username: &str) -> String {
    let clean = username.trim().trim_start_matches('@').to_lowercase();
    format!("https://letterboxd.com/{clean}/rss/")
}

pub fn sync_rss(db: &mut Database, username: &str, xml: &str) -> Result<SyncResult, String> {
    let feed_url = rss_url(username);
    let now = Utc::now().to_rfc3339();
    db.set_meta("self_username", username)?;
    let tx = db.transaction()?;
    let mut added = 0u32;
    let items = parse_items(xml);
    let entries_seen = items.len() as u32;

    for item in items {
        let guid = item.guid.unwrap_or_else(|| {
            row_fingerprint(&[
                ("link", &item.link),
                ("title", &item.title),
                ("published", &item.published),
            ])
        });
        let event_fp = row_fingerprint(&[("guid", &guid), ("feed", &feed_url)]);
        let viewing_key = source_record_key("letterboxd_rss", &feed_url, &event_fp);

        if tx
            .query_row(
                "SELECT id FROM viewings WHERE source_record_key = ?1",
                params![viewing_key],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }

        let movie_fp = row_fingerprint(&[
            ("name", &item.film_title),
            (
                "year",
                &item.year.map(|y| y.to_string()).unwrap_or_default(),
            ),
            ("uri", &item.link),
        ]);
        let movie_key = source_record_key("letterboxd_rss", "film", &movie_fp);
        let meta = SourceMovieMeta {
            poster: item.poster.clone(),
            tmdb_id: item.tmdb_id,
        };
        let smr_id = upsert_source_movie(
            &tx,
            "letterboxd_rss",
            &movie_key,
            &item.film_title,
            item.year,
            &item.link,
            &meta,
        )?;

        tx.execute(
            "INSERT INTO viewings(
              id, source_movie_record_id, source_record_key, occurred_at, published_at,
              observed_at, imported_at, source_type, import_id, diary_entry_id, rewatch, raw_payload
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'letterboxd_rss', NULL, ?8, 0, ?9)",
            params![
                Uuid::new_v4().to_string(),
                smr_id,
                viewing_key,
                item.watched_date,
                item.published,
                now,
                now,
                guid,
                item.raw
            ],
        )
        .map_err(|e| e.to_string())?;

        if let Some(rating) = item.rating {
            let rating_key = format!("{viewing_key}|rating");
            tx.execute(
                "INSERT INTO rating_events(
                  id, source_movie_record_id, source_record_key, rating,
                  occurred_at, published_at, observed_at, imported_at, source_type, import_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'letterboxd_rss', NULL)",
                params![
                    Uuid::new_v4().to_string(),
                    smr_id,
                    rating_key,
                    rating,
                    item.watched_date,
                    item.published,
                    now,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        added += 1;
    }

    Database::rebuild_projections(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;

    let coverage = db.compute_coverage()?;
    Ok(SyncResult {
        username: username.to_string(),
        entries_seen,
        entries_added: added,
        coverage,
    })
}

struct RssItem {
    guid: Option<String>,
    title: String,
    film_title: String,
    year: Option<i32>,
    link: String,
    published: String,
    watched_date: Option<String>,
    rating: Option<f64>,
    poster: Option<String>,
    tmdb_id: Option<i64>,
    raw: String,
}

fn parse_items(xml: &str) -> Vec<RssItem> {
    xml.split("<item>")
        .skip(1)
        .filter_map(|chunk| {
            let body = chunk.split("</item>").next()?;
            let film_title = tag(body, "filmTitle").unwrap_or_default();
            let title = tag(body, "title").unwrap_or_default();
            let name = if film_title.is_empty() {
                title.split(" - ").next()?.trim().to_string()
            } else {
                film_title
            };
            if name.is_empty() {
                return None;
            }
            let year_raw = tag(body, "filmYear");
            let year = year_raw.as_deref().and_then(parse_year).or_else(|| {
                title
                    .chars()
                    .collect::<String>()
                    .split(',')
                    .last()
                    .and_then(|y| parse_year(y.trim()))
            });
            let rating_raw = tag(body, "memberRating");
            let rating = rating_raw
                .as_deref()
                .and_then(|v| v.parse().ok())
                .or_else(|| stars_from_title(&title));
            Some(RssItem {
                guid: tag(body, "guid"),
                title,
                film_title: decode(&name),
                year,
                link: tag(body, "link").unwrap_or_default(),
                published: tag(body, "pubDate").unwrap_or_default(),
                watched_date: tag(body, "watchedDate"),
                rating,
                poster: poster_from_rss_body(body),
                tmdb_id: tmdb_id_from_rss_body(body),
                raw: body.to_string(),
            })
        })
        .collect()
}

fn tag(xml: &str, name: &str) -> Option<String> {
    for marker in [name, &format!("letterboxd:{name}")] {
        let open = format!("<{marker}");
        let xml_lower = xml.to_lowercase();
        let marker_lower = open.to_lowercase();
        let Some(start) = xml_lower.find(&marker_lower) else {
            continue;
        };
        let after = &xml[start..];
        let content_start = after.find('>').map(|i| i + 1)?;
        let rest = &after[content_start..];
        let close = format!("</{marker}>");
        let end = rest.to_lowercase().find(&close.to_lowercase())?;
        return Some(decode(&rest[..end]));
    }
    None
}

fn decode(value: &str) -> String {
    value
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace('★', "")
        .trim()
        .to_string()
}

/// Parse a friend-activity or viewing payload into display title and year.
pub fn parse_activity_payload(raw: &str) -> (String, Option<i32>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(title) = v.get("title").and_then(|t| t.as_str()) {
            let year = v.get("year").and_then(|y| y.as_i64()).map(|y| y as i32);
            return (title.to_string(), year);
        }
    }

    let film_title = tag(raw, "filmTitle").unwrap_or_default();
    let title_tag = tag(raw, "title").unwrap_or_default();
    let name = if film_title.is_empty() {
        title_tag
            .split(" - ")
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        film_title
    };
    let name = decode(&name);
    if name.is_empty() {
        return (raw.trim().to_string(), None);
    }
    let year = tag(raw, "filmYear")
        .and_then(|y| parse_year(&y))
        .or_else(|| {
            title_tag
                .split(',')
                .last()
                .and_then(|y| parse_year(y.trim()))
        });
    (name, year)
}

fn stars_from_title(title: &str) -> Option<f64> {
    let stars: String = title.chars().filter(|&c| c == '★').collect();
    if stars.is_empty() {
        return None;
    }
    let half = title.contains('½') || title.contains("1/2");
    Some(stars.len() as f64 + if half { 0.5 } else { 0.0 })
}

pub fn unique_movie_count(db: &Database) -> Result<u32, String> {
    db.conn()
        .query_row(
            "SELECT COUNT(DISTINCT smr.id) FROM source_movie_records smr
             WHERE EXISTS (SELECT 1 FROM viewings v WHERE v.source_movie_record_id = smr.id)",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn fixture_50_47() -> String {
        let mut xml = String::from(r#"<?xml version="1.0"?><rss><channel>"#);
        for i in 0..47 {
            xml.push_str(&format!(
                r#"<item><title>Film {i} - ★★★★</title><link>https://letterboxd.com/film/f{i}/</link>
                <guid isPermaLink="false">guid-{i}</guid><pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
                <letterboxd:filmTitle>Film {i}</letterboxd:filmTitle><letterboxd:filmYear>2020</letterboxd:filmYear>
                <letterboxd:watchedDate>2024-01-01</letterboxd:watchedDate>
                <letterboxd:memberRating>4.0</letterboxd:memberRating></item>"#
            ));
        }
        for (idx, film) in [(0, 0), (1, 1), (2, 2)] {
            xml.push_str(&format!(
                r#"<item><title>Film {film} - ★★★★★</title><link>https://letterboxd.com/film/f{film}/</link>
                <guid isPermaLink="false">guid-rewatch-{idx}</guid><pubDate>Mon, 02 Jan 2024 00:00:00 GMT</pubDate>
                <letterboxd:filmTitle>Film {film}</letterboxd:filmTitle><letterboxd:filmYear>2020</letterboxd:filmYear>
                <letterboxd:watchedDate>2024-02-01</letterboxd:watchedDate>
                <letterboxd:memberRating>5.0</letterboxd:memberRating>
                <letterboxd:rewatch>Yes</letterboxd:rewatch></item>"#
            ));
        }
        xml.push_str("</channel></rss>");
        xml
    }

    #[test]
    fn parse_activity_payload_reads_rss_item() {
        let raw = r#"<title>Ant-Man, 2015 - ★★★★</title>
        <letterboxd:filmTitle>Ant-Man</letterboxd:filmTitle>
        <letterboxd:filmYear>2015</letterboxd:filmYear>"#;
        let (title, year) = parse_activity_payload(raw);
        assert_eq!(title, "Ant-Man");
        assert_eq!(year, Some(2015));
    }

    #[test]
    fn rss_preserves_50_events_47_movies() {
        let mut db = Database::in_memory().unwrap();
        let xml = fixture_50_47();
        let result = sync_rss(&mut db, "testuser", &xml).unwrap();
        assert_eq!(result.entries_added, 50);
        let (viewings, smr, _) = count_events(&db).unwrap();
        assert_eq!(viewings, 50);
        assert_eq!(smr, 47);
    }
}
