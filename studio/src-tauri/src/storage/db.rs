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
            let _ = self.conn.execute(
                "ALTER TABLE friend_activity ADD COLUMN poster_url TEXT",
                [],
            );
        }
        Ok(())
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
              ) THEN 1 ELSE 0 END,
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
                 )"
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

        Ok(LibraryCoverage {
            unique_movies,
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
        })
    }

    pub fn reset_all_data(&self) -> Result<(), String> {
        self.conn()
            .execute_batch(
                r#"
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
    fn opens_in_memory() {
        let db = Database::in_memory().expect("db");
        assert_eq!(db.get_meta("schema_version").unwrap(), Some("2".into()));
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
}
