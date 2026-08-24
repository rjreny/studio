use serde::Serialize;
use std::path::Path;

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
        .unwrap_or_else(|| path.clone());
    Ok(FileInfo {
        name,
        bytes: meta.len(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![file_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
