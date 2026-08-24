use crate::letterboxd::fingerprint::{row_fingerprint, source_record_key};
use crate::letterboxd::import::upsert_source_movie;
use crate::letterboxd::posters::parse_tmdb_id;
use crate::letterboxd::posters::SourceMovieMeta;
use crate::models::{LegacyLibrary, MigrationResult};
use crate::storage::db::Database;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

const MIGRATION_VERSION: i32 = 1;

pub fn migrate_legacy(
    db: &mut Database,
    legacy: &LegacyLibrary,
) -> Result<MigrationResult, String> {
    db.set_meta("migration_status", "in_progress")?;
    db.set_meta("source_store_version", "studio.json")?;
    db.set_meta("migration_version", &MIGRATION_VERSION.to_string())?;

    if let Some(username) = legacy.username.as_deref() {
        if !username.is_empty() {
            db.set_meta("self_username", username)?;
        }
    }

    let now = Utc::now().to_rfc3339();
    let tx = db.transaction()?;
    let mut imported = 0u32;

    if let Some(films) = &legacy.films {
        for film in films {
            let fp = row_fingerprint(&[
                ("name", &film.name),
                (
                    "year",
                    &film.year.map(|y| y.to_string()).unwrap_or_default(),
                ),
                ("uri", film.uri.as_deref().unwrap_or("")),
                ("watched_date", film.watched_date.as_deref().unwrap_or("")),
                ("kind", "legacy"),
            ]);
            let movie_fp = row_fingerprint(&[
                ("name", &film.name),
                (
                    "year",
                    &film.year.map(|y| y.to_string()).unwrap_or_default(),
                ),
                ("uri", film.uri.as_deref().unwrap_or("")),
            ]);
            let viewing_key = source_record_key("legacy_json", "films", &fp);
            let movie_key = source_record_key("legacy_json", "film", &movie_fp);

            let smr_id = upsert_source_movie(
                &tx,
                "legacy_json",
                &movie_key,
                &film.name,
                film.year,
                film.uri.as_deref().unwrap_or(""),
                &SourceMovieMeta {
                    poster: film.poster.clone(),
                    tmdb_id: film.tmdb_id.as_deref().and_then(parse_tmdb_id),
                },
            )?;

            if tx
                .query_row(
                    "SELECT id FROM viewings WHERE source_record_key = ?1",
                    params![viewing_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .is_none()
            {
                tx.execute(
                    "INSERT INTO viewings(
                      id, source_movie_record_id, source_record_key, occurred_at, published_at,
                      observed_at, imported_at, source_type, import_id, diary_entry_id, rewatch, raw_payload
                    ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5, 'legacy_json', NULL, ?6, ?7, ?8)",
                    params![
                        Uuid::new_v4().to_string(),
                        smr_id,
                        viewing_key,
                        film.watched_date,
                        now,
                        film.uri,
                        if film.rewatch.unwrap_or(false) { 1 } else { 0 },
                        serde_json::json!({ "title": film.name, "year": film.year }).to_string()
                    ],
                )
                .map_err(|e| e.to_string())?;
                imported += 1;
            }

            if let Some(rating) = film.rating {
                let rating_key = format!("{viewing_key}|rating");
                tx.execute(
                    "INSERT OR IGNORE INTO rating_events(
                      id, source_movie_record_id, source_record_key, rating,
                      occurred_at, published_at, observed_at, imported_at, source_type, import_id
                    ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6, 'legacy_json', NULL)",
                    params![
                        Uuid::new_v4().to_string(),
                        smr_id,
                        rating_key,
                        rating,
                        film.watched_date,
                        now
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    Database::rebuild_projections(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;

    let coverage = db.compute_coverage()?;
    let validation = format!(
        "viewings={} unique_movies={}",
        coverage.total_viewings, coverage.unique_movies
    );
    db.set_meta("migration_status", "completed")?;
    db.set_meta("validation_result", &validation)?;

    Ok(MigrationResult {
        status: "completed".into(),
        migration_version: MIGRATION_VERSION,
        validation_result: validation,
        coverage,
    })
}
