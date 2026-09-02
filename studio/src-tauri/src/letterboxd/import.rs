use super::csv::{parse_csv, CsvKind, Record};
use super::fingerprint::{row_fingerprint, source_record_key};
use super::normalize::{normalize_title, parse_year};
use super::posters::{merge_source_movie_metadata_tx, parse_tmdb_id, SourceMovieMeta};
use crate::models::ImportResult;
use crate::storage::db::Database;
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction};
use uuid::Uuid;

pub struct ImportStats {
    pub viewings_added: u32,
    pub ratings_added: u32,
    pub skipped: u32,
}

pub fn import_zip_discovery(
    db: &mut Database,
    discovery: &super::zip::ZipDiscovery,
) -> Result<ImportResult, String> {
    let import_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut tx = db.transaction()?;
    let mut stats = ImportStats {
        viewings_added: 0,
        ratings_added: 0,
        skipped: 0,
    };
    let mut warnings = discovery.warnings.clone();

    tx.execute(
        "INSERT INTO imports(id, source_type, content_hash, imported_at, status, raw_manifest)
         VALUES (?1, 'letterboxd_export', ?2, ?3, 'in_progress', ?4)",
        params![
            import_id,
            discovery.content_hash,
            now,
            serde_json::json!({
                "files": discovery.files.iter().map(|f| &f.relative_path).collect::<Vec<_>>(),
                "unknown": discovery.unknown_paths,
            })
            .to_string()
        ],
    )
    .map_err(|e| e.to_string())?;

    for kind in [
        CsvKind::Diary,
        CsvKind::Watched,
        CsvKind::Ratings,
        CsvKind::Watchlist,
        CsvKind::Reviews,
    ] {
        for file in discovery.files.iter().filter(|file| file.kind == kind) {
            let records = parse_csv(&file.text);
            for record in records {
                match file.kind {
                    CsvKind::Diary | CsvKind::Watched => {
                        if ingest_viewing(
                            &tx,
                            &import_id,
                            &file.relative_path,
                            &record,
                            file.kind,
                            &now,
                            &mut stats,
                        )? {
                            // added
                        }
                    }
                    CsvKind::Ratings => {
                        ingest_rating(
                            &tx,
                            &import_id,
                            &file.relative_path,
                            &record,
                            &now,
                            &mut stats,
                        )?;
                    }
                    CsvKind::Watchlist => {
                        ingest_watchlist_flag(&tx, &import_id, &file.relative_path, &record)?;
                    }
                    CsvKind::Reviews => {
                        ingest_review(&tx, &import_id, &file.relative_path, &record, &mut stats)?;
                    }
                }
            }
        }
    }

    for path in &discovery.unknown_paths {
        tx.execute(
            "INSERT INTO import_entries(id, import_id, source_path, row_number, entity_type, status, warning)
             VALUES (?1, ?2, ?3, NULL, 'unknown', 'skipped', 'unrecognized file')",
            params![Uuid::new_v4().to_string(), import_id, path],
        )
        .map_err(|e| e.to_string())?;
    }

    Database::rebuild_projections(&tx)?;
    tx.execute(
        "UPDATE imports SET status = 'completed' WHERE id = ?1",
        params![import_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;

    let coverage = db.compute_coverage()?;
    let movies = coverage.unique_movies;
    Ok(ImportResult {
        import_id,
        movies,
        viewings: stats.viewings_added,
        ratings: stats.ratings_added,
        skipped: stats.skipped,
        warnings,
        coverage,
    })
}

fn ingest_viewing(
    tx: &Transaction<'_>,
    import_id: &str,
    dataset: &str,
    record: &Record,
    kind: CsvKind,
    imported_at: &str,
    stats: &mut ImportStats,
) -> Result<bool, String> {
    let name = record.get(&["Name", "name"]);
    if name.is_empty() {
        stats.skipped += 1;
        return Ok(false);
    }
    let year = parse_year(&record.get(&["Year", "year"]));
    let watched_date = record.get(&["Watched Date", "Date", "date"]);
    let rating_raw = record.get(&["Rating", "rating"]);
    let uri = record.get(&["Letterboxd URI", "URI", "uri"]);
    let meta = movie_meta_from_record(record);
    let rewatch = record
        .get(&["Rewatch", "rewatch"])
        .eq_ignore_ascii_case("yes");

    if kind == CsvKind::Watched
        && (has_export_viewing(tx, &name, year)?
            || has_rss_viewing(tx, &name, year, &watched_date)?)
    {
        stats.skipped += 1;
        return Ok(false);
    }
    if kind == CsvKind::Diary && has_rss_viewing(tx, &name, year, &watched_date)? {
        stats.skipped += 1;
        return Ok(false);
    }

    let fp = row_fingerprint(&[
        ("name", &name),
        ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
        ("watched_date", &watched_date),
        ("uri", &uri),
        ("kind", kind.as_str()),
        ("rewatch", if rewatch { "yes" } else { "no" }),
    ]);
    let key = source_record_key("letterboxd_export", dataset, &fp);
    let movie_fp = row_fingerprint(&[
        ("name", &name),
        ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
        ("uri", &uri),
    ]);
    let movie_key = source_record_key("letterboxd_export", "film", &movie_fp);

    if tx
        .query_row(
            "SELECT id FROM viewings WHERE source_record_key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        stats.skipped += 1;
        reconcile_source_movie(tx, &movie_key, &name, year)?;
        return Ok(false);
    }

    let smr_id = upsert_source_movie(
        tx,
        "letterboxd_export",
        &movie_key,
        &name,
        year,
        &uri,
        &meta,
    )?;
    let viewing_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO viewings(
          id, source_movie_record_id, source_record_key, occurred_at, published_at,
          observed_at, imported_at, source_type, import_id, diary_entry_id, rewatch, raw_payload
        ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, 'letterboxd_export', ?7, ?8, ?9, ?10)",
        params![
            viewing_id,
            smr_id,
            key,
            if watched_date.is_empty() {
                None::<String>
            } else {
                Some(watched_date.clone())
            },
            imported_at,
            imported_at,
            import_id,
            uri,
            rewatch as i32,
            serde_json::json!({ "title": name, "year": year, "kind": kind.as_str() }).to_string()
        ],
    )
    .map_err(|e| e.to_string())?;

    if !rating_raw.is_empty() {
        if let Ok(rating) = rating_raw.parse::<f64>() {
            let rating_key = format!("{key}|rating");
            if tx
                .query_row(
                    "SELECT id FROM rating_events WHERE source_record_key = ?1",
                    params![rating_key],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .is_none()
            {
                tx.execute(
                    "INSERT INTO rating_events(
                      id, source_movie_record_id, source_record_key, rating,
                      occurred_at, published_at, observed_at, imported_at, source_type, import_id
                    ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, 'letterboxd_export', ?8)",
                    params![
                        Uuid::new_v4().to_string(),
                        smr_id,
                        rating_key,
                        rating,
                        if watched_date.is_empty() {
                            None::<String>
                        } else {
                            Some(watched_date)
                        },
                        imported_at,
                        imported_at,
                        import_id
                    ],
                )
                .map_err(|e| e.to_string())?;
                stats.ratings_added += 1;
            }
        }
    }

    tx.execute(
        "INSERT INTO import_entries(id, import_id, source_path, row_number, entity_type, status, warning)
         VALUES (?1, ?2, ?3, ?4, ?5, 'imported', NULL)",
        params![
            Uuid::new_v4().to_string(),
            import_id,
            dataset,
            record.row_number,
            kind.as_str()
        ],
    )
    .map_err(|e| e.to_string())?;

    stats.viewings_added += 1;
    Ok(true)
}

fn has_export_viewing(
    tx: &Transaction<'_>,
    title: &str,
    year: Option<i32>,
) -> Result<bool, String> {
    tx.query_row(
        "SELECT EXISTS(
           SELECT 1
           FROM viewings v
           JOIN source_movie_records smr ON smr.id = v.source_movie_record_id
           WHERE v.source_type = 'letterboxd_export'
             AND smr.normalized_title = ?1 AND smr.release_year IS ?2
         )",
        params![normalize_title(title), year],
        |row| row.get::<_, i32>(0),
    )
    .map(|n| n != 0)
    .map_err(|e| e.to_string())
}

fn has_rss_viewing(
    tx: &Transaction<'_>,
    title: &str,
    year: Option<i32>,
    watched_date: &str,
) -> Result<bool, String> {
    if watched_date.is_empty() {
        return Ok(false);
    }
    tx.query_row(
        "SELECT EXISTS(
           SELECT 1
           FROM viewings v
           JOIN source_movie_records smr ON smr.id = v.source_movie_record_id
           WHERE v.source_type = 'letterboxd_rss'
             AND smr.normalized_title = ?1 AND smr.release_year IS ?2
             AND v.occurred_at = ?3
         )",
        params![normalize_title(title), year, watched_date],
        |row| row.get::<_, i32>(0),
    )
    .map(|n| n != 0)
    .map_err(|e| e.to_string())
}

fn ingest_rating(
    tx: &Transaction<'_>,
    import_id: &str,
    dataset: &str,
    record: &Record,
    imported_at: &str,
    stats: &mut ImportStats,
) -> Result<(), String> {
    let name = record.get(&["Name", "name"]);
    let rating_raw = record.get(&["Rating", "rating"]);
    if name.is_empty() || rating_raw.is_empty() {
        stats.skipped += 1;
        return Ok(());
    }
    let year = parse_year(&record.get(&["Year", "year"]));
    let uri = record.get(&["Letterboxd URI", "URI", "uri"]);
    let date = record.get(&["Date", "Rated Date", "Watched Date"]);
    let rating: f64 = rating_raw.parse().map_err(|_| "invalid rating")?;
    let meta = movie_meta_from_record(record);

    let fp = row_fingerprint(&[
        ("name", &name),
        ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
        ("date", &date),
        ("rating", &rating_raw),
        ("uri", &uri),
        ("kind", "rating"),
    ]);
    let key = source_record_key("letterboxd_export", dataset, &fp);
    let movie_fp = row_fingerprint(&[
        ("name", &name),
        ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
        ("uri", &uri),
    ]);
    let movie_key = source_record_key("letterboxd_export", "film", &movie_fp);

    if tx
        .query_row(
            "SELECT id FROM rating_events WHERE source_record_key = ?1",
            params![key],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        stats.skipped += 1;
        reconcile_source_movie(tx, &movie_key, &name, year)?;
        return Ok(());
    }

    let smr_id = upsert_source_movie(
        tx,
        "letterboxd_export",
        &movie_key,
        &name,
        year,
        &uri,
        &meta,
    )?;
    tx.execute(
        "INSERT INTO rating_events(
          id, source_movie_record_id, source_record_key, rating,
          occurred_at, published_at, observed_at, imported_at, source_type, import_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, 'letterboxd_export', ?8)",
        params![
            Uuid::new_v4().to_string(),
            smr_id,
            key,
            rating,
            if date.is_empty() {
                None::<String>
            } else {
                Some(date)
            },
            imported_at,
            imported_at,
            import_id
        ],
    )
    .map_err(|e| e.to_string())?;
    stats.ratings_added += 1;
    Ok(())
}

fn ingest_watchlist_flag(
    tx: &Transaction<'_>,
    import_id: &str,
    dataset: &str,
    record: &Record,
) -> Result<(), String> {
    let name = record.get(&["Name", "name"]);
    if name.is_empty() {
        return Ok(());
    }
    let year = parse_year(&record.get(&["Year", "year"]));
    let uri = record.get(&["Letterboxd URI", "URI", "uri"]);
    let meta = movie_meta_from_record(record);
    let fp = row_fingerprint(&[
        ("name", &name),
        ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
        ("uri", &uri),
        ("kind", "watchlist"),
    ]);
    let movie_key = source_record_key("letterboxd_export", "film", &fp);
    let smr_id = upsert_source_movie(
        tx,
        "letterboxd_export",
        &movie_key,
        &name,
        year,
        &uri,
        &meta,
    )?;
    tx.execute(
        "UPDATE source_movie_records SET on_watchlist = 1 WHERE id = ?1",
        params![smr_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO import_entries(id, import_id, source_path, row_number, entity_type, status, warning)
         VALUES (?1, ?2, ?3, ?4, 'watchlist', 'imported', NULL)",
        params![
            Uuid::new_v4().to_string(),
            import_id,
            dataset,
            record.row_number
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn ingest_review(
    tx: &Transaction<'_>,
    import_id: &str,
    dataset: &str,
    record: &Record,
    stats: &mut ImportStats,
) -> Result<(), String> {
    let name = record.get(&["Name", "name"]);
    let review = record.get(&["Review", "review", "Review Text", "Text", "text"]);
    if name.is_empty() || review.is_empty() {
        stats.skipped += 1;
        return Ok(());
    }
    let year = parse_year(&record.get(&["Year", "year"]));
    let uri = record.get(&["Letterboxd URI", "URI", "uri"]);
    let meta = movie_meta_from_record(record);
    let fp = row_fingerprint(&[
        ("name", &name),
        ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
        ("uri", &uri),
    ]);
    let movie_key = source_record_key("letterboxd_export", "film", &fp);
    let smr_id = upsert_source_movie(
        tx,
        "letterboxd_export",
        &movie_key,
        &name,
        year,
        &uri,
        &meta,
    )?;
    tx.execute(
        "UPDATE source_movie_records
         SET raw_identity = json_set(COALESCE(raw_identity, '{}'), '$.review', ?2)
         WHERE id = ?1",
        params![smr_id, review],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO import_entries(id, import_id, source_path, row_number, entity_type, status, warning)
         VALUES (?1, ?2, ?3, ?4, 'reviews', 'imported', NULL)",
        params![Uuid::new_v4().to_string(), import_id, dataset, record.row_number],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn upsert_source_movie(
    tx: &Transaction<'_>,
    source_type: &str,
    source_record_key: &str,
    title: &str,
    year: Option<i32>,
    uri: &str,
    meta: &SourceMovieMeta,
) -> Result<String, String> {
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM source_movie_records WHERE source_record_key = ?1",
            params![source_record_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        merge_source_movie_metadata_tx(
            tx,
            source_record_key,
            meta.poster.as_deref(),
            meta.tmdb_id,
            meta.tmdb_media_type.as_deref(),
        )?;
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    let normalized = normalize_title(title);
    let mut identity = serde_json::json!({ "title": title, "year": year, "uri": uri });
    if let Some(poster) = meta.poster.as_deref().filter(|p| !p.is_empty()) {
        identity["poster"] = serde_json::Value::String(poster.to_string());
    }
    if let Some(tmdb_id) = meta.tmdb_id {
        identity["tmdb_id"] = serde_json::json!(tmdb_id);
    }
    if let Some(kind) = meta.tmdb_media_type.as_deref() {
        identity["tmdb_media_type"] = serde_json::json!(kind);
    }
    tx.execute(
        "INSERT INTO source_movie_records(
          id, source_type, source_record_key, external_id, normalized_title, release_year, raw_identity, cached_poster_url, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            source_type,
            source_record_key,
            if uri.is_empty() { None::<String> } else { Some(uri.to_string()) },
            normalized,
            year,
            identity.to_string(),
            meta.poster.as_deref().filter(|p| !p.is_empty()),
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO movie_links(source_movie_record_id, movie_id, match_state, match_method, confidence, confirmed_at)
         VALUES (?1, NULL, 'unmatched', NULL, NULL, NULL)",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

fn movie_meta_from_record(record: &Record) -> SourceMovieMeta {
    let tmdb_raw = record.get(&["TMDb ID", "TMDB ID", "Tmdb ID", "tmdb_id"]);
    SourceMovieMeta {
        tmdb_id: parse_tmdb_id(&tmdb_raw),
        ..Default::default()
    }
}

fn reconcile_source_movie(
    tx: &Transaction<'_>,
    source_record_key: &str,
    title: &str,
    year: Option<i32>,
) -> Result<(), String> {
    tx.execute(
        "UPDATE source_movie_records
         SET normalized_title = ?2, release_year = ?3,
             raw_identity = json_set(raw_identity, '$.title', ?4)
         WHERE source_record_key = ?1",
        params![source_record_key, normalize_title(title), year, title],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn count_events(db: &Database) -> Result<(u32, u32, u32), String> {
    Ok((
        db.count_table("viewings")?,
        db.count_table("source_movie_records")?,
        db.count_table("rating_events")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::letterboxd::zip::discover_zip_bytes;

    fn diary_csv(rows: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("diary.csv", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            format!("Name,Year,Watched Date,Letterboxd URI,Rating,Rewatch\n{rows}").as_bytes(),
        )
        .unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn diary_and_watched_csv() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("watched.csv", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            b"Date,Name,Year,Letterboxd URI\n2025-01-01,Tuner,2025,/film/tuner/\n",
        )
        .unwrap();
        zip.start_file("diary.csv", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            b"Name,Year,Watched Date,Letterboxd URI,Rating,Rewatch\nTuner,2025,2025-01-01,/film/tuner/,4.5,No\n",
        )
        .unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn identical_zip_no_duplicate_events() {
        let bytes = diary_csv("Inception,2010,2020-01-01,/film/inception/,4.5,No");
        let mut db = Database::in_memory().unwrap();
        let d1 = discover_zip_bytes(&bytes).unwrap();
        import_zip_discovery(&mut db, &d1).unwrap();
        let d2 = discover_zip_bytes(&bytes).unwrap();
        let result = import_zip_discovery(&mut db, &d2).unwrap();
        assert_eq!(result.viewings, 0);
        let (viewings, _, _) = count_events(&db).unwrap();
        assert_eq!(viewings, 1);
    }

    #[test]
    fn overlapping_zip_adds_only_new_rows() {
        let mut db = Database::in_memory().unwrap();
        let b1 = diary_csv("Inception,2010,2020-01-01,/film/inception/,4.5,No");
        import_zip_discovery(&mut db, &discover_zip_bytes(&b1).unwrap()).unwrap();
        let b2 = diary_csv(
            "Inception,2010,2020-01-01,/film/inception/,4.5,No\n\
             Inception,2010,2024-06-01,/film/inception/,5,Yes",
        );
        let result = import_zip_discovery(&mut db, &discover_zip_bytes(&b2).unwrap()).unwrap();
        assert_eq!(result.viewings, 1);
        let (viewings, _, _) = count_events(&db).unwrap();
        assert_eq!(viewings, 2);
    }

    #[test]
    fn watched_summary_does_not_duplicate_diary_history() {
        let mut db = Database::in_memory().unwrap();
        let result = import_zip_discovery(
            &mut db,
            &discover_zip_bytes(&diary_and_watched_csv()).unwrap(),
        )
        .unwrap();
        assert_eq!(result.viewings, 1);
        assert_eq!(count_events(&db).unwrap().0, 1);
    }

    #[test]
    fn failed_import_rolls_back() {
        let db = Database::in_memory().unwrap();
        let bad = b"not a zip".to_vec();
        assert!(discover_zip_bytes(&bad).is_err());
        let (viewings, smr, _) = count_events(&db).unwrap();
        assert_eq!(viewings, 0);
        assert_eq!(smr, 0);
    }

    #[test]
    fn standalone_ratings_and_reviews_are_imported_without_diary_logs() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("ratings.csv", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            b"Name,Year,Rating,Letterboxd URI\nInception,2010,4.5,/film/inception/\n",
        )
        .unwrap();
        zip.start_file("reviews.csv", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            b"Name,Year,Review,Letterboxd URI\nInception,2010,Beautifully shot and tightly paced,/film/inception/\n",
        )
        .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let mut db = Database::in_memory().unwrap();
        let result = import_zip_discovery(&mut db, &discover_zip_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(result.viewings, 0);
        assert_eq!(result.ratings, 1);

        let (viewings, movies, ratings) = count_events(&db).unwrap();
        assert_eq!((viewings, movies, ratings), (0, 1, 1));

        let (rating, review): (f64, String) = db
            .conn()
            .query_row(
                "SELECT re.rating, json_extract(smr.raw_identity, '$.review')
                 FROM rating_events re
                 JOIN source_movie_records smr ON smr.id = re.source_movie_record_id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(rating, 4.5);
        assert_eq!(review, "Beautifully shot and tightly paced");
    }
}
