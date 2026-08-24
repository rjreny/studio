mod app_log;
mod catalog;
mod commands;
mod letterboxd;
mod migration;
mod models;
mod queries;
mod storage;

use commands::init_state;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tauri::Manager;

#[derive(Serialize)]
struct FileInfo {
    name: String,
    bytes: u64,
}

#[tauri::command]
fn file_info(path: String) -> Result<FileInfo, String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(path);
    Ok(FileInfo {
        name,
        bytes: meta.len(),
    })
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_zip_texts(path: String) -> Result<HashMap<String, String>, String> {
    let file = File::open(&path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = Path::new(entry.name())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !name.to_ascii_lowercase().ends_with(".csv") {
            continue;
        }
        let mut buf = String::new();
        if entry.read_to_string(&mut buf).is_err() {
            continue;
        }
        out.insert(name.to_ascii_lowercase(), buf);
    }
    Ok(out)
}

fn allowed_fetch(url: &str) -> bool {
    url.starts_with("https://letterboxd.com/")
        && url.chars().all(|c| c.is_ascii() && !c.is_control())
}

pub fn fetch_url(url: &str) -> Result<String, String> {
    if !allowed_fetch(url) {
        return Err("blocked host".into());
    }
    ureq::get(url)
        .set("User-Agent", "Studio/0.1 (local film app)")
        .set(
            "Accept",
            "application/rss+xml, application/xml, text/xml, */*",
        )
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn fetch_text(url: String) -> Result<String, String> {
    fetch_url(&url)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = init_state(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            file_info,
            read_text_file,
            read_zip_texts,
            fetch_text,
            commands::get_coverage,
            commands::get_session,
            commands::set_self_username,
            commands::get_install_info,
            commands::reset_all_data,
            commands::launch_uninstaller,
            commands::library_get,
            commands::film_get,
            commands::home_get,
            commands::import_export_zip,
            commands::import_get_diagnostics,
            commands::sync_self,
            commands::sync_friends,
            commands::import_friend_usernames,
            commands::film_set_rating,
            commands::migrate_from_legacy,
            commands::tmdb_set_key,
            commands::tmdb_clear_key,
            commands::tmdb_has_key,
            commands::tmdb_key_status,
            commands::tmdb_enrich,
            commands::list_friends,
            commands::update_preflight,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
