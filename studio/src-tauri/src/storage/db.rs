use super::schema::{MIGRATION_SQL, SCHEMA_VERSION};
use crate::models::{AppSession, LibraryCoverage};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use std::time::Duration;

const FILM_KEY: &str =
    "COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), ''))";

fn apply_connection_pragmas(conn: &Connection) -> Result<(), String> {
    conn.busy_timeout(Duration::from_millis(5000))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        apply_connection_pragmas(&conn)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        apply_connection_pragmas(&conn)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        self.conn
            .execute_batch(MIGRATION_SQL)
            .map_err(|e| e.to_string())?;
        self.apply_schema_patches()?;
        self.set_meta("schema_version", &SCHEMA_VERSION.to_string())
    }

    fn apply_schema_patches(&self) -> Result<(), String> {
        let version: i32 = self
            .get_meta("schema_version")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        if version < 2 {
            let _ = self.conn.execute(
                "ALTER TABLE source_movie_records ADD COLUMN cached_poster_url TEXT",
                [],
            );
            let _ = self.conn.execute(
                "ALTER TABLE source_movie_records ADD COLUMN poster_fetch_failed INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = self
                .conn
                .execute("ALTER TABLE friend_activity ADD COLUMN poster_url TEXT", []);
        }
        if version < 3 {
            let _ = self
                .conn
                .execute("ALTER TABLE movies ADD COLUMN tagline TEXT", []);
            let _ = self
                .conn
                .execute("ALTER TABLE movies ADD COLUMN collection_name TEXT", []);
            let _ = self
                .conn
                .execute("ALTER TABLE movies ADD COLUMN collection_json TEXT", []);
        }
        if version < 4 {
            let _ = self
                .conn
                .execute("ALTER TABLE movies ADD COLUMN keywords_json TEXT", []);
            let _ = self
                .conn
                .execute("ALTER TABLE movies ADD COLUMN credits_json TEXT", []);
            let _ = self.conn.execute(
                "CREATE TABLE IF NOT EXISTS person_credits (
                  person_id INTEGER PRIMARY KEY,
                  credits_json TEXT NOT NULL,
                  fetched_at TEXT NOT NULL
                )",
                [],
            );
        }
        if version < 5 {
            let _ = self.conn.execute(
                r#"CREATE TABLE IF NOT EXISTS taste_feedback (
                  content_key TEXT PRIMARY KEY,
                  tmdb_id INTEGER NOT NULL,
                  media_kind TEXT NOT NULL DEFAULT 'movie',
                  action TEXT NOT NULL,
                  reason TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                )"#,
                [],
            );
        }
        if version < 6 {
            let _ = self.conn.execute(
                r#"CREATE TABLE IF NOT EXISTS taste_run_snapshot (
                  id INTEGER PRIMARY KEY CHECK (id = 1),
                  algorithm_version TEXT NOT NULL,
                  profile_fingerprint TEXT NOT NULL,
                  library_state_fingerprint TEXT NOT NULL,
                  candidate_input_fingerprint TEXT NOT NULL,
                  scoring_fingerprint TEXT NOT NULL,
                  narrative_key TEXT NOT NULL,
                  catalog_valid_until TEXT NOT NULL,
                  scored_pool_json TEXT NOT NULL,
                  narrative_json TEXT NOT NULL,
                  created_at TEXT NOT NULL
                )"#,
                [],
            );
        }
        if version < 7 {
            let _ = self.conn.execute(
                r#"CREATE TABLE IF NOT EXISTS taste_embeddings (
                  tmdb_id INTEGER NOT NULL,
                  model TEXT NOT NULL,
                  content_hash TEXT NOT NULL,
                  dimension INTEGER NOT NULL,
                  vector_json TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  PRIMARY KEY (tmdb_id, model)
                )"#,
                [],
            );
            let _ = self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_taste_embeddings_model ON taste_embeddings(model)",
                [],
            );
        }
        if version < 9 {
            self.reconcile_duplicate_history()?;
        }
        if version < 10 {
            let _ = self.conn.execute(
                "ALTER TABLE taste_feedback ADD COLUMN suppressed_until TEXT",
                [],
            );
            self.conn
                .execute_batch(
                    r#"
                    CREATE TABLE IF NOT EXISTS taste_recommendation_exposures (
                      id TEXT PRIMARY KEY,
                      run_id TEXT NOT NULL,
                      tmdb_id INTEGER NOT NULL,
                      title TEXT NOT NULL,
                      snapshot_json TEXT NOT NULL,
                      prior_candidate_exposures INTEGER NOT NULL DEFAULT 0,
                      created_at TEXT NOT NULL
                    );
                    CREATE TABLE IF NOT EXISTS taste_exposure_features (
                      exposure_id TEXT NOT NULL REFERENCES taste_recommendation_exposures(id),
                      feature_key TEXT NOT NULL,
                      PRIMARY KEY (exposure_id, feature_key)
                    );
                    CREATE TABLE IF NOT EXISTS taste_feedback_events (
                      id TEXT PRIMARY KEY,
                      exposure_id TEXT NOT NULL REFERENCES taste_recommendation_exposures(id),
                      tmdb_id INTEGER NOT NULL,
                      action TEXT NOT NULL,
                      reason TEXT,
                      target_feature_key TEXT,
                      mood_scope TEXT,
                      mood_fallback INTEGER NOT NULL DEFAULT 0,
                      requested_adjustments_json TEXT NOT NULL DEFAULT '[]',
                      applied_adjustments_json TEXT NOT NULL DEFAULT '[]',
                      feature_snapshot_json TEXT NOT NULL DEFAULT '{}',
                      feedback_signal_version TEXT NOT NULL,
                      expires_at TEXT,
                      created_at TEXT NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_taste_exposures_tmdb ON taste_recommendation_exposures(tmdb_id);
                    CREATE INDEX IF NOT EXISTS idx_taste_exposure_features_key ON taste_exposure_features(feature_key);
                    CREATE INDEX IF NOT EXISTS idx_taste_feedback_events_exposure ON taste_feedback_events(exposure_id);
                    "#,
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn reconcile_duplicate_history(&self) -> Result<(), String> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute(
            r#"
            DELETE FROM viewings
            WHERE id IN (
              SELECT id FROM (
                SELECT
                  v.id,
                  ROW_NUMBER() OVER (
                    PARTITION BY
                      smr.normalized_title,
                      smr.release_year,
                      COALESCE(NULLIF(v.occurred_at, ''), NULLIF(v.published_at, ''))
                    ORDER BY
                      CASE v.source_type
                        WHEN 'letterboxd_export' THEN 0
                        WHEN 'legacy_json' THEN 1
                        ELSE 2
                      END,
                      v.observed_at,
                      v.id
                  ) AS duplicate_rank
                FROM viewings v
                JOIN source_movie_records smr ON smr.id = v.source_movie_record_id
                WHERE COALESCE(NULLIF(v.occurred_at, ''), NULLIF(v.published_at, '')) IS NOT NULL
              )
              WHERE duplicate_rank > 1
            )
            "#,
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            r#"
            DELETE FROM viewings
            WHERE id IN (
              SELECT summary.id
              FROM viewings summary
              JOIN source_movie_records smr ON smr.id = summary.source_movie_record_id
              WHERE summary.source_type = 'letterboxd_export'
                AND summary.diary_entry_id LIKE 'https://boxd.it/%'
                AND LENGTH(SUBSTR(summary.diary_entry_id, LENGTH('https://boxd.it/') + 1)) = 4
                AND EXISTS (
                  SELECT 1
                  FROM viewings other
                  JOIN source_movie_records other_smr ON other_smr.id = other.source_movie_record_id
                  WHERE other.id != summary.id
                    AND other_smr.normalized_title = smr.normalized_title
                    AND other_smr.release_year IS smr.release_year
                    AND DATE(other.occurred_at) BETWEEN DATE(summary.occurred_at, '-1 day')
                                                     AND DATE(summary.occurred_at, '+1 day')
                )
            )
            "#,
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            r#"
            UPDATE source_movie_records
            SET cached_poster_url = NULL, poster_fetch_failed = 0
            WHERE id IN (
              SELECT smr.id
              FROM source_movie_records smr
              JOIN movie_links ml ON ml.source_movie_record_id = smr.id
              JOIN movies m ON m.id = ml.movie_id
              WHERE ml.match_method IN ('tmdb_search', 'propagated')
                AND smr.normalized_title != LOWER(TRIM(m.canonical_title))
            )
            "#,
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            r#"
            UPDATE movie_links
            SET movie_id = NULL, match_state = 'unmatched', match_method = NULL,
                confidence = NULL, confirmed_at = NULL
            WHERE match_method IN ('tmdb_search', 'propagated')
              AND EXISTS (
                SELECT 1
                FROM source_movie_records smr
                JOIN movies m ON m.id = movie_links.movie_id
                WHERE smr.id = movie_links.source_movie_record_id
                  AND smr.normalized_title != LOWER(TRIM(m.canonical_title))
              )
            "#,
            [],
        )
        .map_err(|e| e.to_string())?;
        Self::rebuild_projections(&tx)?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn transaction(&mut self) -> Result<Transaction<'_>, String> {
        self.conn.transaction().map_err(|e| e.to_string())
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO app_meta(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn count_table(&self, table: &str) -> Result<u32, String> {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        self.conn
            .query_row(&sql, [], |row| row.get::<_, i64>(0))
            .map(|n| n as u32)
            .map_err(|e| e.to_string())
    }

    pub fn rebuild_projections(tx: &Transaction<'_>) -> Result<(), String> {
        tx.execute("DELETE FROM user_movie_state", [])
            .map_err(|e| e.to_string())?;
        tx.execute(
            r#"
            INSERT INTO user_movie_state (
              source_movie_record_id, movie_id, watched, watchlist, liked,
              current_rating, last_watched_at, projection_updated_at
            )
            SELECT
              smr.id,
              ml.movie_id,
              CASE WHEN EXISTS (
                SELECT 1 FROM viewings v WHERE v.source_movie_record_id = smr.id
              ) OR EXISTS (
                SELECT 1 FROM rating_events re WHERE re.source_movie_record_id = smr.id
              ) OR json_extract(smr.raw_identity, '$.review') IS NOT NULL THEN 1 ELSE 0 END,
              COALESCE(smr.on_watchlist, 0),
              0,
              (
                SELECT re.rating FROM rating_events re
                WHERE re.source_movie_record_id = smr.id
                ORDER BY COALESCE(re.occurred_at, re.observed_at) DESC
                LIMIT 1
              ),
              (
                SELECT v.occurred_at FROM viewings v
                WHERE v.source_movie_record_id = smr.id
                ORDER BY COALESCE(v.occurred_at, v.observed_at) DESC
                LIMIT 1
              ),
              ?1
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            "#,
            params![Utc::now().to_rfc3339()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn compute_coverage(&self) -> Result<LibraryCoverage, String> {
        let unique_movies: u32 = self
            .conn
            .query_row(
                &format!(
                    "SELECT COUNT(DISTINCT {FILM_KEY})
                 FROM source_movie_records smr
                 LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
                 WHERE EXISTS (
                   SELECT 1 FROM viewings v WHERE v.source_movie_record_id = smr.id
                 ) OR EXISTS (
                   SELECT 1 FROM rating_events re WHERE re.source_movie_record_id = smr.id
                 ) OR json_extract(smr.raw_identity, '$.review') IS NOT NULL
                 "
                ),
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        let total_viewings = self.count_table("viewings")?;
        let rating_events = self.count_table("rating_events")?;
        let unresolved_movies: u32 = self
            .conn
            .query_row(
                &format!(
                    "SELECT COUNT(DISTINCT {FILM_KEY})
                 FROM source_movie_records smr
                 LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
                 WHERE (ml.match_state IS NULL OR ml.match_state IN ('unmatched', 'ambiguous'))
                 AND (
                   EXISTS (SELECT 1 FROM viewings v WHERE v.source_movie_record_id = smr.id)
                   OR EXISTS (SELECT 1 FROM rating_events re WHERE re.source_movie_record_id = smr.id)
                   OR json_extract(smr.raw_identity, '$.review') IS NOT NULL
                   OR smr.on_watchlist = 1
                 )"
                ),
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        let has_export: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM imports WHERE source_type = 'letterboxd_export' AND status = 'completed'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|e| e.to_string())?;

        let has_rss: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM viewings WHERE source_type = 'letterboxd_rss'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|e| e.to_string())?;

        let source = match (has_export, has_rss) {
            (true, true) => "mixed",
            (true, false) => "export",
            (false, true) => "rss",
            (false, false) => "none",
        }
        .to_string();

        let last_full_import = self
            .conn
            .query_row(
                "SELECT imported_at FROM imports
                 WHERE source_type = 'letterboxd_export' AND status = 'completed'
                 ORDER BY imported_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        let mut warnings = Vec::new();
        if has_rss && !has_export {
            warnings.push(
                "Only RSS data present — full history requires an official Letterboxd export."
                    .into(),
            );
        }

        let watchlist_movies: u32 = self
            .conn
            .query_row(
                &format!(
                    "SELECT COUNT(DISTINCT {FILM_KEY})
                 FROM source_movie_records smr
                 LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
                 LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
                 WHERE ums.watchlist = 1 OR smr.on_watchlist = 1"
                ),
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        Ok(LibraryCoverage {
            unique_movies,
            watchlist_movies,
            total_viewings,
            rating_events,
            unresolved_movies,
            source,
            full_history_available: has_export,
            rss_window_limit: if has_rss { Some(50) } else { None },
            last_full_import,
            warnings,
        })
    }

    pub fn get_session(&self) -> Result<AppSession, String> {
        let coverage = self.compute_coverage()?;
        let self_username = self
            .get_meta("self_username")?
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty());
        let friend_count = self.count_table("friends")?;
        let friend_activity = self.count_table("friend_activity")?;
        let imports = self.count_table("imports")?;
        let has_setup = self_username.is_some()
            || friend_count > 0
            || friend_activity > 0
            || imports > 0
            || coverage.total_viewings > 0
            || coverage.unique_movies > 0;
        Ok(AppSession {
            self_username,
            friend_count,
            has_setup,
            coverage,
            last_rss_sync_at: self.get_meta("last_rss_sync_at")?,
            rss_paused_until: self.get_meta("rss_backoff_until")?,
        })
    }

    pub fn remove_friend(&self, id: &str) -> Result<String, String> {
        let username: String = self
            .conn()
            .query_row(
                "SELECT username FROM friends WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Friend not found".to_string())?;
        self.conn()
            .execute(
                "DELETE FROM friend_activity WHERE friend_id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        self.conn()
            .execute("DELETE FROM friends WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(username)
    }

    pub fn reset_all_data(&self) -> Result<(), String> {
        self.conn()
            .execute_batch(
                r#"
                DELETE FROM taste_run_snapshot;
                DELETE FROM taste_embeddings;
                DELETE FROM taste_feedback_events;
                DELETE FROM taste_exposure_features;
                DELETE FROM taste_recommendation_exposures;
                DELETE FROM taste_feedback;
                DELETE FROM person_credits;
                DELETE FROM friend_activity;
                DELETE FROM friends;
                DELETE FROM import_entries;
                DELETE FROM imports;
                DELETE FROM rating_events;
                DELETE FROM viewings;
                DELETE FROM user_movie_state;
                DELETE FROM movie_aliases;
                DELETE FROM movie_links;
                DELETE FROM movies;
                DELETE FROM source_movie_records;
                DELETE FROM app_meta;
                "#,
            )
            .map_err(|e| e.to_string())?;
        self.set_meta("schema_version", &SCHEMA_VERSION.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconciliation_keeps_one_viewing_and_unlinks_wrong_search_results() {
        let db = Database::in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO movies(id, canonical_title, release_year, tmdb_id) VALUES ('wrong', 'The Piano Tuner', 2025, 1571662)",
                [],
            )
            .unwrap();
        for (id, source, method, occurred_at, diary_entry_id) in [
            (
                "diary",
                "letterboxd_export",
                "tmdb_search",
                "2025-01-01",
                "https://boxd.it/fMUQ7R",
            ),
            (
                "rss",
                "letterboxd_rss",
                "propagated",
                "2025-01-01",
                "letterboxd-review-1446480335",
            ),
            (
                "summary",
                "letterboxd_export",
                "propagated",
                "2025-01-02",
                "https://boxd.it/icFU",
            ),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_movie_records(
                       id, source_type, source_record_key, normalized_title, release_year,
                       raw_identity, cached_poster_url, created_at
                     ) VALUES (?1, ?2, ?1, 'tuner', 2025, '{\"title\":\"Tuner\"}', 'wrong-poster', '2025-01-01T00:00:00Z')",
                    params![id, source],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO movie_links(source_movie_record_id, movie_id, match_state, match_method)
                     VALUES (?1, 'wrong', 'confirmed', ?2)",
                    params![id, method],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO viewings(
                       id, source_movie_record_id, source_record_key, occurred_at, observed_at,
                       source_type, diary_entry_id
                     ) VALUES (?1, ?1, ?1, ?2, '2025-01-01T00:00:00Z', ?3, ?4)",
                    params![id, occurred_at, source, diary_entry_id],
                )
                .unwrap();
        }

        db.reconcile_duplicate_history().unwrap();

        assert_eq!(db.count_table("viewings").unwrap(), 1);
        let unmatched: u32 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM movie_links WHERE movie_id IS NULL AND match_state = 'unmatched'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unmatched, 3);
        let stale_posters: u32 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM source_movie_records WHERE cached_poster_url IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stale_posters, 0);
    }
    #[test]
    fn opens_in_memory() {
        let db = Database::in_memory().expect("db");
        assert_eq!(db.get_meta("schema_version").unwrap(), Some("10".into()));
    }

    #[test]
    fn file_db_uses_wal() {
        let path = std::env::temp_dir().join(format!("studio-wal-{}.db", uuid::Uuid::new_v4()));
        let db = Database::open(&path).expect("db");
        let mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal_mode");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_file_name(format!(
            "{}-wal",
            path.file_name().unwrap().to_string_lossy()
        )));
        let _ = std::fs::remove_file(path.with_file_name(format!(
            "{}-shm",
            path.file_name().unwrap().to_string_lossy()
        )));
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn remove_friend_drops_only_that_friend_activity() {
        let db = Database::in_memory().expect("db");
        db.conn()
            .execute(
                "INSERT INTO friends(id, username, enabled) VALUES ('f1', 'ada', 1), ('f2', 'bee', 1)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO friend_activity(id, friend_id, source_record_key, activity_type)
                 VALUES ('a1', 'f1', 'k1', 'diary'), ('a2', 'f2', 'k2', 'diary')",
                [],
            )
            .unwrap();
        assert_eq!(db.remove_friend("f1").unwrap(), "ada");
        let remaining: Vec<(String, String)> = {
            let mut stmt = db
                .conn()
                .prepare("SELECT id, username FROM friends ORDER BY username")
                .unwrap();
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        assert_eq!(remaining, vec![("f2".into(), "bee".into())]);
        let activity: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM friend_activity", [], |row| row.get(0))
            .unwrap();
        let leftover_friend: String = db
            .conn()
            .query_row("SELECT friend_id FROM friend_activity", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(activity, 1);
        assert_eq!(leftover_friend, "f2");
        assert_eq!(db.remove_friend("missing").unwrap_err(), "Friend not found");
    }

    #[test]
    fn schema_v10_has_feedback_snapshot_embedding_and_observation_tables() {
        let db = Database::in_memory().expect("db");
        assert_eq!(db.get_meta("schema_version").unwrap(), Some("10".into()));
        let feedback: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'taste_feedback'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let snap: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'taste_run_snapshot'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let embeddings: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'taste_embeddings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let exposures: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'taste_recommendation_exposures'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let events: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'taste_feedback_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(feedback, 1);
        assert_eq!(snap, 1);
        assert_eq!(embeddings, 1);
        assert_eq!(exposures, 1);
        assert_eq!(events, 1);
        db.reset_all_data().unwrap();
        let leftover: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM taste_feedback", [], |row| row.get(0))
            .unwrap();
        assert_eq!(leftover, 0);
    }
}
