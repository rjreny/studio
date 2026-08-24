use crate::catalog::tmdb::library_item_from_tmdb_value;
use crate::letterboxd::posters::{backdrop_url, poster_from_rss_body, poster_url};
use crate::letterboxd::rss::parse_activity_payload;
use crate::models::{
    FilmDetail, FriendActivityItem, HomeViewModel, LibraryItem, LibraryPage,
    LibraryQuery, ViewingHistoryItem,
};
use crate::storage::db::Database;
use rusqlite::{params, params_from_iter, OptionalExtension};
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

pub fn parse_tmdb_ref(id: &str) -> Option<i64> {
    id.strip_prefix("tmdb:")?.parse().ok().filter(|n| *n > 0)
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
    if let Some(detail) = get_library_film(db, id)? {
        return Ok(detail);
    }
    get_catalog_film(db, id)?.ok_or_else(|| "Film not found".to_string())
}

pub fn resolve_source_movie_ids(db: &Database, id: &str) -> Result<Vec<String>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT smr.id
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            WHERE smr.id = ?1
               OR ml.movie_id = ?1
               OR (smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), '')) = ?1
               OR CAST(m.tmdb_id AS TEXT) = ?1
               OR ('tmdb:' || CAST(m.tmdb_id AS TEXT)) = ?1
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![id], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn get_library_film(db: &Database, id: &str) -> Result<Option<FilmDetail>, String> {
    let row = db
        .conn()
        .query_row(
            r#"
            SELECT
              COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), '')),
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
              m.vote_average,
              m.vote_count,
              m.reviews_json,
              m.cast_json,
              m.crew_json,
              m.similar_json,
              m.tmdb_id,
              m.tagline,
              m.collection_name,
              m.collection_json
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
            WHERE smr.id = ?1
               OR ml.movie_id = ?1
               OR (smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), '')) = ?1
               OR CAST(m.tmdb_id AS TEXT) = ?1
               OR ('tmdb:' || CAST(m.tmdb_id AS TEXT)) = ?1
            ORDER BY CASE WHEN ml.match_state = 'confirmed' THEN 0 ELSE 1 END, ums.last_watched_at DESC
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
                    row.get::<_, Option<f64>>(11)?,
                    row.get::<_, Option<i32>>(12)?,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, Option<i64>>(17)?,
                    row.get::<_, Option<String>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Option<String>>(20)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Ok(None);
    };

    let source_ids = resolve_source_movie_ids(db, id)?;
    let title = parse_raw_title(&row.1);
    let your_history = viewing_history(db, &source_ids)?;
    let crew = json_vec(row.15.as_deref());
    let similar = relink_catalog_items(db, parse_related(row.16.as_deref()));
    let collection = relink_catalog_items(db, parse_related(row.20.as_deref()));

    Ok(Some(FilmDetail {
        id: row.0,
        title: title.clone(),
        year: row.2,
        current_rating: row.3,
        poster: poster_url(row.4.clone()),
        backdrop: backdrop_url(row.5.clone()),
        overview: row.6,
        runtime: row.7,
        genres: json_vec(row.8.as_deref()),
        match_state: row.9,
        source_identity: row.10,
        your_history,
        friends: get_friend_activity_for_movie(db, &title, row.2)?,
        tmdb_id: row.17,
        tmdb_vote_average: row.11,
        tmdb_vote_count: row.12,
        tmdb_reviews: json_vec(row.13.as_deref()),
        tagline: row.18.filter(|s| !s.is_empty()),
        directors: directors_from_crew(&crew),
        cast: json_vec(row.14.as_deref()),
        crew,
        collection_name: row.19.filter(|s| !s.is_empty()),
        collection,
        similar,
        collection_hydrated: row.20.is_some(),
    }))
}

fn get_catalog_film(db: &Database, id: &str) -> Result<Option<FilmDetail>, String> {
    let tmdb_id = parse_tmdb_ref(id);
    let row = db
        .conn()
        .query_row(
            r#"
            SELECT id, canonical_title, release_year, poster_path, backdrop_path, overview, runtime,
                   genres_json, vote_average, vote_count, reviews_json, cast_json, crew_json,
                   similar_json, tmdb_id, tagline, collection_name, collection_json
            FROM movies
            WHERE id = ?1
               OR CAST(tmdb_id AS TEXT) = ?1
               OR ('tmdb:' || CAST(tmdb_id AS TEXT)) = ?1
            LIMIT 1
            "#,
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i32>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i32>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<f64>>(8)?,
                    row.get::<_, Option<i32>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<i64>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, Option<String>>(17)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Ok(None);
    };

    let crew = json_vec(row.12.as_deref());
    let catalog_id = tmdb_id
        .or(row.14)
        .map(|n| format!("tmdb:{n}"))
        .unwrap_or(row.0);
    Ok(Some(FilmDetail {
        id: catalog_id,
        title: row.1,
        year: row.2,
        current_rating: None,
        poster: poster_url(row.3.clone()),
        backdrop: backdrop_url(row.4.clone()),
        overview: row.5,
        runtime: row.6,
        genres: json_vec(row.7.as_deref()),
        match_state: "catalog".into(),
        source_identity: "tmdb".into(),
        your_history: Vec::new(),
        friends: Vec::new(),
        tmdb_id: row.14,
        tmdb_vote_average: row.8,
        tmdb_vote_count: row.9,
        tmdb_reviews: json_vec(row.10.as_deref()),
        tagline: row.15.filter(|s| !s.is_empty()),
        directors: directors_from_crew(&crew),
        cast: json_vec(row.11.as_deref()),
        crew,
        collection_name: row.16.filter(|s| !s.is_empty()),
        collection: parse_related(row.17.as_deref()),
        similar: parse_related(row.13.as_deref()),
        collection_hydrated: row.17.is_some(),
    }))
}

fn viewing_history(db: &Database, source_ids: &[String]) -> Result<Vec<ViewingHistoryItem>, String> {
    if source_ids.is_empty() {
        return Ok(Vec::new());
    }
    let marks = source_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, occurred_at, published_at, rewatch,
              (SELECT rating FROM rating_events re WHERE re.source_movie_record_id = v.source_movie_record_id
               AND COALESCE(re.occurred_at, re.observed_at) = COALESCE(v.occurred_at, v.observed_at) LIMIT 1),
              source_type
         FROM viewings v WHERE v.source_movie_record_id IN ({marks})
         ORDER BY COALESCE(occurred_at, observed_at) DESC"
    );
    let mut stmt = db.conn().prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(source_ids.iter()), |row| {
            Ok(ViewingHistoryItem {
                id: row.get(0)?,
                occurred_at: row.get(1)?,
                published_at: row.get(2)?,
                rewatch: row.get::<_, i32>(3)? == 1,
                rating: row.get(4)?,
                source: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn directors_from_crew(crew: &[String]) -> Vec<String> {
    crew.iter()
        .filter_map(|entry| entry.strip_suffix(" (Director)").map(str::to_string))
        .collect()
}

fn parse_related(raw: Option<&str>) -> Vec<LibraryItem> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    if let Ok(items) = serde_json::from_str::<Vec<LibraryItem>>(raw) {
        if items.iter().any(|item| !item.title.is_empty()) {
            return items;
        }
    }
    serde_json::from_str::<Vec<serde_json::Value>>(raw)
        .ok()
        .map(|vals| {
            vals.iter()
                .filter_map(library_item_from_tmdb_value)
                .collect()
        })
        .unwrap_or_default()
}

fn relink_catalog_items(db: &Database, items: Vec<LibraryItem>) -> Vec<LibraryItem> {
    items
        .into_iter()
        .map(|mut item| {
            if let Some(tmdb_id) = parse_tmdb_ref(&item.id) {
                if let Ok(Some(key)) = library_key_for_tmdb(db, tmdb_id) {
                    item.id = key;
                }
            }
            item
        })
        .collect()
}

fn library_key_for_tmdb(db: &Database, tmdb_id: i64) -> Result<Option<String>, String> {
    db.conn()
        .query_row(
            &format!(
                "SELECT {FILM_KEY}
                 FROM source_movie_records smr
                 JOIN movie_links ml ON ml.source_movie_record_id = smr.id
                 JOIN movies m ON m.id = ml.movie_id
                 WHERE m.tmdb_id = ?1
                 LIMIT 1"
            ),
            params![tmdb_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
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
            WHERE LOWER(fa.raw_payload) LIKE ?1
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
            limit: Some(36),
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

    #[test]
    fn get_film_opens_matched_library_id() {
        use crate::letterboxd::import::upsert_source_movie;
        use crate::letterboxd::posters::SourceMovieMeta;
        use rusqlite::params;
        use uuid::Uuid;

        let mut db = Database::in_memory().expect("db");
        let tx = db.transaction().expect("tx");
        let movie_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO movies (id, canonical_title, release_year, tmdb_id, poster_path, backdrop_path, similar_json, collection_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                movie_id,
                "Ant-Man",
                2015,
                102899,
                "/poster.jpg",
                "/backdrop.jpg",
                r#"[{"id":27205,"title":"Inception","release_date":"2010-07-16","poster_path":"/inception.jpg"}]"#,
                "[]"
            ],
        )
        .expect("movie");
        let smr = upsert_source_movie(
            &tx,
            "letterboxd_export",
            "export|ant-man",
            "Ant-Man",
            Some(2015),
            "https://letterboxd.com/film/ant-man/",
            &SourceMovieMeta::default(),
        )
        .expect("smr");
        tx.execute(
            "UPDATE movie_links SET movie_id = ?2, match_state = 'confirmed' WHERE source_movie_record_id = ?1",
            params![smr, movie_id],
        )
        .expect("link");
        tx.execute(
            "INSERT INTO viewings (
              id, source_movie_record_id, source_record_key, observed_at, source_type, rewatch
            ) VALUES (?1, ?2, ?3, ?4, 'letterboxd_export', 0)",
            params![
                Uuid::new_v4().to_string(),
                smr,
                format!("viewing-{smr}"),
                "2024-01-01T00:00:00Z"
            ],
        )
        .expect("viewing");
        Database::rebuild_projections(&tx).expect("rebuild");
        tx.commit().expect("commit");

        let page = get_library(
            &db,
            &LibraryQuery {
                search: None,
                sort: None,
                filter: None,
                limit: Some(10),
                offset: Some(0),
            },
        )
        .expect("library");
        assert_eq!(page.items.len(), 1);
        let detail = get_film(&db, &page.items[0].id).expect("film");
        assert_eq!(detail.title, "Ant-Man");
        assert!(detail.backdrop.unwrap().contains("/original/"));
        assert_eq!(detail.similar.len(), 1);
        assert_eq!(detail.similar[0].title, "Inception");
    }

    #[test]
    fn get_film_opens_unmatched_title_year_key() {
        use crate::letterboxd::import::upsert_source_movie;
        use crate::letterboxd::posters::SourceMovieMeta;
        use rusqlite::params;
        use uuid::Uuid;

        let mut db = Database::in_memory().expect("db");
        let tx = db.transaction().expect("tx");
        let smr = upsert_source_movie(
            &tx,
            "letterboxd_export",
            "export|mystery",
            "Mystery Film",
            Some(1999),
            "https://letterboxd.com/film/mystery-film/",
            &SourceMovieMeta::default(),
        )
        .expect("smr");
        tx.execute(
            "INSERT INTO viewings (
              id, source_movie_record_id, source_record_key, observed_at, source_type, rewatch
            ) VALUES (?1, ?2, ?3, ?4, 'letterboxd_export', 0)",
            params![
                Uuid::new_v4().to_string(),
                smr,
                format!("viewing-{smr}"),
                "2024-01-01T00:00:00Z"
            ],
        )
        .expect("viewing");
        Database::rebuild_projections(&tx).expect("rebuild");
        tx.commit().expect("commit");

        let page = get_library(
            &db,
            &LibraryQuery {
                search: None,
                sort: None,
                filter: None,
                limit: Some(10),
                offset: Some(0),
            },
        )
        .expect("library");
        assert_eq!(page.items[0].id.contains(':'), true);
        let detail = get_film(&db, &page.items[0].id).expect("unmatched film");
        assert_eq!(detail.title, "Mystery Film");
        assert_eq!(detail.your_history.len(), 1);
    }
}
