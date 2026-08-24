use crate::catalog::tmdb;
use crate::letterboxd::import::import_zip_discovery;
use crate::letterboxd::rss::{rss_url, sync_rss};
use crate::letterboxd::zip::discover_zip;
use crate::migration::migrate_legacy;
use crate::models::{
    AppSession, FilmDetail, FriendSyncResult, HomeViewModel, ImportDiagnostics, ImportResult,
    ImportSummary, InstallInfo, LegacyLibrary, LibraryCoverage, LibraryPage, LibraryQuery,
    MigrationResult, SetRatingInput, SyncResult,
};
use crate::queries::{get_film, get_friend_feed, get_home, get_library};
use crate::storage::db::Database;
use chrono::Utc;
use rusqlite::params;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

pub struct AppState {
    pub db: Mutex<Database>,
}

fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("studio.db"))
}

pub fn init_state(app: &AppHandle) -> Result<AppState, String> {
    let path = db_path(app)?;
    let db = Database::open(&path)?;
    Ok(AppState { db: Mutex::new(db) })
}

#[tauri::command]
pub fn get_coverage(state: State<'_, AppState>) -> Result<LibraryCoverage, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.compute_coverage()
}

#[tauri::command]
pub fn get_session(state: State<'_, AppState>) -> Result<AppSession, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_session()
}

#[tauri::command]
pub fn set_self_username(username: String, state: State<'_, AppState>) -> Result<(), String> {
    let clean = username.trim().trim_start_matches('@').to_string();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    if clean.is_empty() {
        db.conn()
            .execute("DELETE FROM app_meta WHERE key = 'self_username'", [])
            .map_err(|e| e.to_string())?;
    } else {
        db.set_meta("self_username", &clean)?;
    }
    Ok(())
}

fn install_kind(exe: &std::path::Path) -> &'static str {
    let path = exe.to_string_lossy().to_lowercase();
    if path.contains("target\\debug")
        || path.contains("target/debug")
        || path.contains("target\\release")
        || path.contains("target/release")
        || path.contains("cursor-sandbox")
    {
        "dev"
    } else if path.contains("\\programs\\") || path.contains("/programs/") {
        "installed"
    } else {
        "portable"
    }
}

fn find_uninstaller(exe: &std::path::Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("uninstall") && name.ends_with(".exe") {
            return Some(entry.path());
        }
    }
    None
}

#[tauri::command]
pub fn get_install_info(app: AppHandle) -> Result<InstallInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = db_path(&app)?;
    let exe = std::env::current_exe().ok();
    let kind = exe
        .as_ref()
        .map(|p| install_kind(p).to_string())
        .unwrap_or_else(|| "unknown".into());
    let uninstaller = exe.as_ref().and_then(|p| find_uninstaller(p));
    Ok(InstallInfo {
        version: app
            .config()
            .version
            .clone()
            .unwrap_or_else(|| "0.0.0".into()),
        install_kind: kind,
        app_data_dir: data_dir.to_string_lossy().into_owned(),
        database_path: db_path.to_string_lossy().into_owned(),
        executable_path: exe.map(|p| p.to_string_lossy().into_owned()),
        uninstaller_path: uninstaller.map(|p| p.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
pub fn reset_all_data(state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.reset_all_data()
}

#[tauri::command]
pub fn launch_uninstaller(app: AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller =
        find_uninstaller(&exe).ok_or_else(|| "No uninstaller found for this build".to_string())?;
    std::process::Command::new(&uninstaller)
        .spawn()
        .map_err(|e| e.to_string())?;
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn library_get(query: LibraryQuery, state: State<'_, AppState>) -> Result<LibraryPage, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_library(&db, &query)
}

#[tauri::command]
pub fn film_get(id: String, state: State<'_, AppState>) -> Result<FilmDetail, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_film(&db, &id)
}

#[tauri::command]
pub fn home_get(state: State<'_, AppState>) -> Result<HomeViewModel, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_home(&db)
}

#[tauri::command]
pub fn import_export_zip(path: String, state: State<'_, AppState>) -> Result<ImportResult, String> {
    let discovery = discover_zip(&path)?;
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let result = import_zip_discovery(&mut db, &discovery)?;
    let _ = tmdb::enrich_catalog(&db);
    Ok(result)
}

#[tauri::command]
pub fn import_get_diagnostics(state: State<'_, AppState>) -> Result<ImportDiagnostics, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = db
        .conn()
        .prepare("SELECT id, content_hash, imported_at, status FROM imports ORDER BY imported_at DESC LIMIT 20")
        .map_err(|e| e.to_string())?;
    let imports = stmt
        .query_map([], |row| {
            Ok(ImportSummary {
                id: row.get(0)?,
                content_hash: row.get(1)?,
                imported_at: row.get(2)?,
                status: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    let coverage = db.compute_coverage()?;
    Ok(ImportDiagnostics {
        imports,
        warnings: coverage.warnings,
    })
}

#[tauri::command]
pub fn sync_self(username: String, state: State<'_, AppState>) -> Result<SyncResult, String> {
    let clean = username.trim().trim_start_matches('@').to_string();
    let url = rss_url(&clean);
    let xml = crate::fetch_url(&url)?;
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let result = sync_rss(&mut db, &clean, &xml)?;
    let _ = tmdb::enrich_catalog(&db);
    Ok(result)
}

#[tauri::command]
pub fn sync_friends(state: State<'_, AppState>) -> Result<FriendSyncResult, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let friends: Vec<(String, String)> = {
        let mut stmt = db
            .conn()
            .prepare("SELECT id, username FROM friends WHERE enabled = 1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    let mut entries_added = 0u32;
    let mut errors = Vec::new();
    for (friend_id, username) in friends {
        let url = rss_url(&username);
        match crate::fetch_url(&url) {
            Ok(xml) => {
                if let Err(err) = sync_friend_rss(&mut db, &friend_id, &username, &xml) {
                    errors.push(format!("@{username}: {err}"));
                    let _ = db.conn().execute(
                        "UPDATE friends SET last_sync_error = ?2 WHERE id = ?1",
                        params![friend_id, err],
                    );
                } else {
                    entries_added += 1;
                    let _ = db.conn().execute(
                        "UPDATE friends SET last_sync_at = ?2, last_sync_error = NULL WHERE id = ?1",
                        params![friend_id, Utc::now().to_rfc3339()],
                    );
                }
            }
            Err(err) => errors.push(format!("@{username}: {err}")),
        }
    }
    Ok(FriendSyncResult {
        friends_synced: entries_added,
        entries_added,
        errors,
    })
}

fn sync_friend_rss(
    db: &mut Database,
    friend_id: &str,
    username: &str,
    xml: &str,
) -> Result<(), String> {
    use crate::letterboxd::fingerprint::{row_fingerprint, source_record_key};
    use crate::letterboxd::import::upsert_source_movie;
    use crate::letterboxd::normalize::parse_year;
    use crate::letterboxd::posters::{
        poster_from_rss_body, tmdb_id_from_rss_body, SourceMovieMeta,
    };
    use rusqlite::OptionalExtension;

    let feed_url = rss_url(username);
    let now = Utc::now().to_rfc3339();
    let mut tx = db.transaction()?;

    for item in xml
        .split("<item>")
        .skip(1)
        .filter_map(|c| c.split("</item>").next())
    {
        let film_title = extract_tag(item, "filmTitle").unwrap_or_default();
        let title = extract_tag(item, "title").unwrap_or_default();
        let name = if film_title.is_empty() {
            title.split(" - ").next().unwrap_or("").trim().to_string()
        } else {
            film_title
        };
        if name.is_empty() {
            continue;
        }
        let guid = extract_tag(item, "guid").unwrap_or_else(|| {
            row_fingerprint(&[("link", &extract_tag(item, "link").unwrap_or_default())])
        });
        let event_fp = row_fingerprint(&[("guid", &guid), ("feed", &feed_url)]);
        let activity_key = source_record_key("letterboxd_rss", &feed_url, &event_fp);
        if tx
            .query_row(
                "SELECT id FROM friend_activity WHERE source_record_key = ?1",
                params![activity_key],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let year = extract_tag(item, "filmYear").and_then(|y| parse_year(&y));
        let link = extract_tag(item, "link").unwrap_or_default();
        let movie_fp = row_fingerprint(&[
            ("name", &name),
            ("year", &year.map(|y| y.to_string()).unwrap_or_default()),
            ("uri", &link),
        ]);
        let movie_key = source_record_key("letterboxd_rss", "film", &movie_fp);
        let meta = SourceMovieMeta {
            poster: poster_from_rss_body(item),
            tmdb_id: tmdb_id_from_rss_body(item),
        };
        let _smr =
            upsert_source_movie(&tx, "letterboxd_rss", &movie_key, &name, year, &link, &meta)?;
        let rating: Option<f64> = extract_tag(item, "memberRating").and_then(|v| v.parse().ok());
        let poster = poster_from_rss_body(item);
        tx.execute(
            "INSERT INTO friend_activity(
              id, friend_id, source_movie_record_id, source_record_key, activity_type,
              published_at, watched_at, rating, review, source_guid, raw_payload, poster_url
            ) VALUES (?1, ?2, NULL, ?3, 'diary', ?4, ?5, ?6, NULL, ?7, ?8, ?9)",
            params![
                Uuid::new_v4().to_string(),
                friend_id,
                activity_key,
                extract_tag(item, "pubDate"),
                extract_tag(item, "watchedDate"),
                rating,
                guid,
                item,
                poster
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

fn extract_tag(xml: &str, name: &str) -> Option<String> {
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

#[tauri::command]
pub fn import_friend_usernames(text: String, state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let mut added = 0u32;
    for line in text.lines() {
        let username = line
            .trim()
            .trim_start_matches('@')
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if username.is_empty()
            || !username
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let exists: bool = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM friends WHERE username = ?1",
                params![username],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(|e| e.to_string())?;
        if exists {
            continue;
        }
        db.conn()
            .execute(
                "INSERT INTO friends(id, username, enabled) VALUES (?1, ?2, 1)",
                params![Uuid::new_v4().to_string(), username],
            )
            .map_err(|e| e.to_string())?;
        added += 1;
    }
    Ok(added)
}

#[tauri::command]
pub fn film_set_rating(
    input: SetRatingInput,
    state: State<'_, AppState>,
) -> Result<FilmDetail, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let smr_id: String = db
        .conn()
        .query_row(
            "SELECT id FROM source_movie_records WHERE id = ?1
             UNION SELECT source_movie_record_id FROM movie_links WHERE movie_id = ?1 LIMIT 1",
            params![input.id, input.id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let rating_key = format!("manual|{smr_id}|{now}");
    db.conn()
        .execute(
            "INSERT INTO rating_events(
              id, source_movie_record_id, source_record_key, rating,
              occurred_at, published_at, observed_at, imported_at, source_type, import_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?5, NULL, 'manual', NULL)",
            params![
                Uuid::new_v4().to_string(),
                smr_id,
                rating_key,
                input.rating,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
    let mut tx = db.transaction()?;
    Database::rebuild_projections(&tx)?;
    tx.commit().map_err(|e| e.to_string())?;
    get_film(&db, &input.id)
}

#[tauri::command]
pub fn migrate_from_legacy(
    legacy: LegacyLibrary,
    state: State<'_, AppState>,
) -> Result<MigrationResult, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    migrate_legacy(&mut db, &legacy)
}

#[tauri::command]
pub fn tmdb_set_key(key: String, state: State<'_, AppState>) -> Result<(), String> {
    tmdb::set_api_key(&key)?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "UPDATE movie_links SET match_state = 'unmatched'
             WHERE match_state = 'ambiguous'",
            [],
        )
        .map_err(|e| e.to_string())?;
    let _ = tmdb::enrich_catalog(&db);
    Ok(())
}

#[tauri::command]
pub fn tmdb_clear_key() -> Result<(), String> {
    tmdb::clear_api_key()
}

#[tauri::command]
pub fn tmdb_has_key() -> Result<bool, String> {
    Ok(tmdb::get_api_key()?
        .filter(|k| !k.trim().is_empty())
        .is_some())
}

#[tauri::command]
pub fn tmdb_enrich(state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "UPDATE movie_links SET match_state = 'unmatched'
             WHERE match_state = 'ambiguous'",
            [],
        )
        .map_err(|e| e.to_string())?;
    tmdb::enrich_catalog(&db)
}

#[tauri::command]
pub fn list_friends(
    state: State<'_, AppState>,
) -> Result<Vec<(String, String, Option<String>, Option<String>)>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT id, username, last_sync_at, last_sync_error FROM friends ORDER BY username",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

#[derive(Serialize)]
pub struct UpdatePreflight {
    pub signing_configured: bool,
    pub endpoint: String,
    pub http_status: Option<u16>,
    pub reachable: bool,
    pub message: String,
    pub update_available: bool,
    pub available_version: Option<String>,
}

fn updater_config(app: &AppHandle) -> (String, String) {
    let plugins = app.config().plugins.clone();
    let updater = plugins.0.get("updater").cloned().unwrap_or_default();
    let pubkey = updater
        .get("pubkey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let endpoint = updater
        .get("endpoints")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    (pubkey, endpoint)
}

fn update_preflight_message(signing_configured: bool, status: u16) -> String {
    if !signing_configured {
        if status == 404 {
            return "You're on the current build. Signed auto-updates aren't configured for this install yet."
                .to_string();
        }
        return format!(
            "Signed auto-updates aren't configured yet (release server HTTP {})",
            status
        );
    }
    if status == 204 || status == 404 {
        return "You're up to date".to_string();
    }
    if status >= 200 && status < 300 {
        return "Update available — click Update to download and install".to_string();
    }
    format!("Release server returned HTTP {}", status)
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn finish_update_preflight(
    signing_configured: bool,
    endpoint: String,
    status: u16,
    body: &str,
    current_version: &str,
) -> UpdatePreflight {
    let mut update_available = false;
    let mut available_version = None;
    let mut message = update_preflight_message(signing_configured, status);

    if status >= 200 && status < 300 && !body.is_empty() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(release) = json.get("version").and_then(|v| v.as_str()) {
                let current = normalize_version(current_version);
                let release_norm = normalize_version(release);
                if release_norm != current {
                    update_available = true;
                    available_version = Some(release.to_string());
                    message = if signing_configured {
                        format!("Update available: {}", release)
                    } else {
                        format!(
                            "Release {} is published — run signer:generate and set pubkey to install",
                            release
                        )
                    };
                } else {
                    message = "You're up to date".to_string();
                }
            }
        }
    }

    UpdatePreflight {
        signing_configured,
        endpoint,
        http_status: Some(status),
        reachable: true,
        message,
        update_available,
        available_version,
    }
}

#[tauri::command]
pub fn update_preflight(app: AppHandle) -> Result<UpdatePreflight, String> {
    let (pubkey, endpoint) = updater_config(&app);
    let signing_configured = !pubkey.is_empty();
    let current_version = app
        .config()
        .version
        .as_deref()
        .unwrap_or("0.0.0");

    if endpoint.is_empty() {
        return Ok(UpdatePreflight {
            signing_configured,
            endpoint,
            http_status: None,
            reachable: false,
            message: "No update endpoint configured in tauri.conf.json".to_string(),
            update_available: false,
            available_version: None,
        });
    }

    let response = ureq::get(&endpoint)
        .set("User-Agent", "Studio/0.1 (update check)")
        .set("Accept", "application/json")
        .call();

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.into_string().unwrap_or_default();
            Ok(finish_update_preflight(
                signing_configured,
                endpoint,
                status,
                &body,
                &current_version,
            ))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Ok(finish_update_preflight(
                signing_configured,
                endpoint,
                code,
                &body,
                &current_version,
            ))
        }
        Err(e) => Ok(UpdatePreflight {
            signing_configured,
            endpoint,
            http_status: None,
            reachable: false,
            message: format!("Could not reach release server: {}", e),
            update_available: false,
            available_version: None,
        }),
    }
}
