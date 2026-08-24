use crate::letterboxd::posters::poster_from_rss_body;
use crate::letterboxd::rss::parse_activity_payload;
use crate::models::{
    FilmDetail, FriendActivityItem, HomeViewModel, LibraryCoverage, LibraryItem, LibraryPage,
    LibraryQuery, ViewingHistoryItem,
};
use crate::storage::db::Database;
use rusqlite::{params, OptionalExtension};
use serde_json;

const FILM_KEY: &str =
    "COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), ''))";

pub fn get_library(db: &Database, query: &LibraryQuery) -> Result<LibraryPage, String> {
    let search = query.search.as_deref().unwrap_or("").trim().to_lowercase();
    let limit = query.limit.unwrap_or(200) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let sort = query.sort.as_deref().unwrap_or("recent");

    let order = match sort {
        "title" => "title_raw ASC",
        "rating" => "current_rating DESC",
        "year" => "year DESC",
        _ => "last_watched_at DESC",
    };

    let search_clause = if search.is_empty() {
        String::new()
    } else {
        " AND smr.normalized_title LIKE ?1 ".to_string()
    };

    let limit_clause = if search.is_empty() {
        format!(" ORDER BY {order} LIMIT ?1 OFFSET ?2 ")
    } else {
        format!(" ORDER BY {order} LIMIT ?2 OFFSET ?3 ")
    };

    let sql = format!(
        r#"
        WITH film_entries AS (
          SELECT
            {film_key} AS film_key,
            smr.id AS source_id,
            COALESCE(m.canonical_title, smr.raw_identity) AS title_raw,
            COALESCE(m.release_year, smr.release_year) AS year,
            ums.current_rating,
            m.poster_path AS poster,
            m.backdrop_path AS backdrop,
            m.overview AS overview,
            smr.cached_poster_url,
            json_extract(smr.raw_identity, '$.poster') AS identity_poster,
            ums.watched,
            ums.watchlist,
            ums.liked,
            (SELECT COUNT(*) FROM viewings v WHERE v.source_movie_record_id = smr.id) AS source_viewing_count,
            COALESCE(ml.match_state, 'unmatched') AS match_state,
            smr.source_type,
            ums.last_watched_at
          FROM source_movie_records smr
          LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
          LEFT JOIN movies m ON m.id = ml.movie_id
          LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
          WHERE (ums.watched = 1 OR ums.watchlist = 1 OR ums.current_rating IS NOT NULL)
          {search_clause}
        ),
        aggregated AS (
          SELECT
            film_key,
            SUM(source_viewing_count) AS viewing_count,
            MAX(watched) AS watched,
            MAX(watchlist) AS watchlist,
            MAX(liked) AS liked
          FROM film_entries
          GROUP BY film_key
        ),
        primary_row AS (
          SELECT
            fe.*,
            (
              SELECT COALESCE(m3.poster_path, smr3.cached_poster_url, json_extract(smr3.raw_identity, '$.poster'))
              FROM source_movie_records smr3
              LEFT JOIN movie_links ml3 ON ml3.source_movie_record_id = smr3.id
              LEFT JOIN movies m3 ON m3.id = ml3.movie_id
              WHERE COALESCE(ml3.movie_id, smr3.normalized_title || ':' || IFNULL(CAST(smr3.release_year AS TEXT), '')) = fe.film_key
                AND COALESCE(m3.poster_path, smr3.cached_poster_url, json_extract(smr3.raw_identity, '$.poster')) IS NOT NULL
                AND TRIM(COALESCE(m3.poster_path, smr3.cached_poster_url, json_extract(smr3.raw_identity, '$.poster'), '')) != ''
              LIMIT 1
            ) AS resolved_poster,
            ROW_NUMBER() OVER (
              PARTITION BY fe.film_key
              ORDER BY fe.last_watched_at DESC, fe.source_id
            ) AS rn
          FROM film_entries fe
        )
        SELECT
          p.film_key AS id,
          p.title_raw,
          p.year,
          p.current_rating,
          COALESCE(p.resolved_poster, p.poster, p.cached_poster_url, p.identity_poster) AS poster,
          a.watched,
          a.watchlist,
          a.liked,
          a.viewing_count,
          p.match_state,
          p.source_type,
          p.last_watched_at,
          p.backdrop,
          p.overview
        FROM primary_row p
        JOIN aggregated a ON a.film_key = p.film_key
        WHERE p.rn = 1
        {limit_clause}
        "#,
        film_key = FILM_KEY
    );

    let mut stmt = db.conn().prepare(&sql).map_err(|e| e.to_string())?;
    let rows = if search.is_empty() {
        stmt.query_map(params![limit, offset], map_library_item)
    } else {
        stmt.query_map(
            params![format!("%{search}%"), limit, offset],
            map_library_item,
        )
    }
    .map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| e.to_string())?);
    }

    let total: u32 = db
        .conn()
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT {FILM_KEY})
             FROM source_movie_records smr
             LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
             LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
             WHERE ums.watched = 1 OR ums.watchlist = 1 OR ums.current_rating IS NOT NULL"
            ),
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(LibraryPage {
        items,
        total,
        coverage: db.compute_coverage()?,
    })
}

fn poster_url(path: Option<String>) -> Option<String> {
    tmdb_image_url(path, "w500")
}

fn backdrop_url(path: Option<String>) -> Option<String> {
    tmdb_image_url(path, "w1280")
}

fn tmdb_image_url(path: Option<String>, size: &str) -> Option<String> {
    path.filter(|p| !p.is_empty()).map(|p| {
        if p.starts_with("http") {
            p
        } else {
            format!("https://image.tmdb.org/t/p/{size}{p}")
        }
    })
}

fn map_library_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    let raw_title: String = row.get(1)?;
    let title = parse_raw_title(&raw_title);
    Ok(LibraryItem {
        id: row.get(0)?,
        title,
        year: row.get(2)?,
        current_rating: row.get(3)?,
        poster: poster_url(row.get(4)?),
        watched: row.get::<_, i32>(5)? == 1,
        watchlist: row.get::<_, i32>(6)? == 1,
        liked: row.get::<_, i32>(7)? == 1,
        viewing_count: row.get::<_, i64>(8)? as u32,
        match_state: row.get(9)?,
        source_type: row.get(10)?,
        last_watched_at: row.get(11)?,
        backdrop: backdrop_url(row.get(12)?),
        overview: row.get(13)?,
    })
}

fn parse_raw_title(raw: &str) -> String {
    parse_activity_payload(raw).0
}

pub fn get_film(db: &Database, id: &str) -> Result<FilmDetail, String> {
    let row = db
        .conn()
        .query_row(
            r#"
            SELECT
              COALESCE(ml.movie_id, smr.id),
              COALESCE(m.canonical_title, smr.raw_identity),
              COALESCE(m.release_year, smr.release_year),
              ums.current_rating,
              COALESCE(m.poster_path, smr.cached_poster_url, json_extract(smr.raw_identity, '$.poster')),
              m.backdrop_path,
              m.overview,
              m.runtime,
              m.genres_json,
              COALESCE(ml.match_state, 'unmatched'),
              smr.source_type,
              smr.raw_identity,
              m.vote_average,
              m.vote_count,
              m.reviews_json,
              m.cast_json,
              m.crew_json,
              m.similar_json
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
            WHERE smr.id = ?1 OR ml.movie_id = ?1
            LIMIT 1
            "#,
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i32>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<i32>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<f64>>(12)?,
                    row.get::<_, Option<i32>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, Option<String>>(17)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Film not found".to_string())?;

    let smr_id: String = db
        .conn()
        .query_row(
            "SELECT id FROM source_movie_records WHERE id = ?1
             UNION SELECT source_movie_record_id FROM movie_links WHERE movie_id = ?1 LIMIT 1",
            params![id, id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let mut history_stmt = db
        .conn()
        .prepare(
            "SELECT id, occurred_at, published_at, rewatch,
              (SELECT rating FROM rating_events re WHERE re.source_movie_record_id = v.source_movie_record_id
               AND COALESCE(re.occurred_at, re.observed_at) = COALESCE(v.occurred_at, v.observed_at) LIMIT 1),
              source_type
             FROM viewings v WHERE v.source_movie_record_id = ?1
             ORDER BY COALESCE(occurred_at, observed_at) DESC",
        )
        .map_err(|e| e.to_string())?;

    let your_history = history_stmt
        .query_map(params![smr_id], |row| {
            Ok(ViewingHistoryItem {
                id: row.get(0)?,
                occurred_at: row.get(1)?,
                published_at: row.get(2)?,
                rewatch: row.get::<_, i32>(3)? == 1,
                rating: row.get(4)?,
                source: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let genres = json_vec(row.8.as_deref());
    let tmdb_reviews = json_vec(row.14.as_deref());
    let cast = json_vec(row.15.as_deref());
    let crew = json_vec(row.16.as_deref());
    let similar: Vec<LibraryItem> = row
        .17
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(FilmDetail {
        id: row.0,
        title: parse_raw_title(&row.1),
        year: row.2,
        current_rating: row.3,
        poster: poster_url(row.4.clone()),
        backdrop: backdrop_url(row.5.clone()),
        overview: row.6,
        runtime: row.7,
        genres,
        match_state: row.9,
        source_identity: row.10,
        your_history,
        friends: get_friend_activity_for_movie(db, &parse_raw_title(&row.1), row.2)?,
        tmdb_vote_average: row.12,
        tmdb_vote_count: row.13,
        tmdb_reviews,
        cast,
        crew,
        similar,
    })
}

fn json_vec(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

fn get_friend_activity_for_movie(
    db: &Database,
    title: &str,
    year: Option<i32>,
) -> Result<Vec<FriendActivityItem>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT f.username, fa.raw_payload, fa.rating, fa.review, fa.watched_at, fa.published_at
            FROM friend_activity fa
            JOIN friends f ON f.id = fa.friend_id
            WHERE fa.raw_payload LIKE ?1
            ORDER BY COALESCE(fa.watched_at, fa.published_at) DESC
            LIMIT 20
            "#,
        )
        .map_err(|e| e.to_string())?;
    let needle = format!("%{}%", title.to_lowercase());
    let rows = stmt
        .query_map(params![needle], |row| {
            Ok(FriendActivityItem {
                username: row.get(0)?,
                title: title.to_string(),
                year,
                rating: row.get(2)?,
                review: row.get(3)?,
                watched_at: row.get(4)?,
                published_at: row.get(5)?,
                poster: None,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_home(db: &Database) -> Result<HomeViewModel, String> {
    let page = get_library(
        db,
        &LibraryQuery {
            search: None,
            sort: Some("recent".into()),
            filter: None,
            limit: Some(12),
            offset: Some(0),
        },
    )?;
    let top = get_library(
        db,
        &LibraryQuery {
            search: None,
            sort: Some("rating".into()),
            filter: None,
            limit: Some(12),
            offset: Some(0),
        },
    )?;
    Ok(HomeViewModel {
        coverage: db.compute_coverage()?,
        recent: page.items,
        top_rated: top.items,
        friend_feed: get_friend_feed(db, 30)?,
    })
}

pub fn get_friend_feed(db: &Database, limit: u32) -> Result<Vec<FriendActivityItem>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT f.username, fa.raw_payload, fa.rating, fa.review, fa.watched_at, fa.published_at, fa.poster_url
            FROM friend_activity fa
            JOIN friends f ON f.id = fa.friend_id
            ORDER BY COALESCE(fa.watched_at, fa.published_at) DESC
            LIMIT ?1
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            let raw: String = row.get(1)?;
            let stored_poster: Option<String> = row.get(6)?;
            let (title, year) = parse_activity_payload(&raw);
            Ok(FriendActivityItem {
                username: row.get(0)?,
                title,
                year,
                rating: row.get(2)?,
                review: row.get(3)?,
                watched_at: row.get(4)?,
                published_at: row.get(5)?,
                poster: stored_poster.or_else(|| poster_from_rss_body(&raw)),
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LibraryQuery;
    use crate::storage::db::Database;

    #[test]
    fn library_query_without_search_binds_limit_offset() {
        let db = Database::in_memory().expect("db");
        let page = get_library(
            &db,
            &LibraryQuery {
                search: None,
                sort: Some("recent".into()),
                filter: None,
                limit: Some(10),
                offset: Some(0),
            },
        )
        .expect("library page");
        assert_eq!(page.items.len(), 0);
        assert_eq!(page.total, 0);
    }

    #[test]
    fn library_deduplicates_linked_source_records() {
        use crate::letterboxd::import::upsert_source_movie;
        use crate::letterboxd::posters::SourceMovieMeta;
        use rusqlite::params;
        use uuid::Uuid;

        let mut db = Database::in_memory().expect("db");
        let tx = db.transaction().expect("tx");
        let movie_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO movies (id, canonical_title, release_year) VALUES (?1, ?2, ?3)",
            params![movie_id, "Ant-Man", 2015],
        )
        .expect("movie");

        let smr_a = upsert_source_movie(
            &tx,
            "letterboxd_export",
            "export|ant-man",
            "Ant-Man",
            Some(2015),
            "https://letterboxd.com/film/ant-man/",
            &SourceMovieMeta::default(),
        )
        .expect("smr a");
        let smr_b = upsert_source_movie(
            &tx,
            "letterboxd_rss",
            "rss|ant-man",
            "Ant-Man",
            Some(2015),
            "https://letterboxd.com/film/ant-man/",
            &SourceMovieMeta::default(),
        )
        .expect("smr b");

        for smr_id in [&smr_a, &smr_b] {
            tx.execute(
                "UPDATE movie_links SET movie_id = ?2, match_state = 'confirmed'
                 WHERE source_movie_record_id = ?1",
                params![smr_id, movie_id],
            )
            .expect("link");
            tx.execute(
                "INSERT INTO viewings (
                  id, source_movie_record_id, source_record_key, observed_at, source_type, rewatch
                ) VALUES (?1, ?2, ?3, ?4, 'letterboxd_export', 0)",
                params![
                    Uuid::new_v4().to_string(),
                    smr_id,
                    format!("viewing-{smr_id}"),
                    "2024-01-01T00:00:00Z"
                ],
            )
            .expect("viewing");
        }

        Database::rebuild_projections(&tx).expect("rebuild");
        tx.commit().expect("commit");

        let page = get_library(
            &db,
            &LibraryQuery {
                search: None,
                sort: None,
                filter: None,
                limit: Some(100),
                offset: Some(0),
            },
        )
        .expect("library");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].viewing_count, 2);
    }
}
