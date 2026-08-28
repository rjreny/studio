use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub fn log_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("studio.log"))
}

pub fn write(app: &AppHandle, line: impl AsRef<str>) {
    if let Ok(path) = log_path(app) {
        write_to(&path, line.as_ref());
    }
}

pub fn write_to(path: &Path, line: &str) {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let formatted = format!("[{ts}] {line}");
    eprintln!("[studio] {line}");
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{formatted}"));
}

pub fn write_json(app: &AppHandle, folder: &str, filename: &str, body: &str) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join(folder);
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(filename);
    fs::write(&path, body).ok()?;
    Some(path)
}
