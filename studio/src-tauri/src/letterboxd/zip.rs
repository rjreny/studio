use super::csv::{classify_csv, is_letterboxd_list_export_path, parse_headers, CsvKind};
use super::fingerprint::content_hash;
use std::io::Read;
use std::path::{Component, Path};
use zip::ZipArchive;

pub const MAX_ENTRIES: usize = 500;
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ZipCsvFile {
    pub relative_path: String,
    pub kind: CsvKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ZipDiscovery {
    pub content_hash: String,
    pub files: Vec<ZipCsvFile>,
    pub unknown_paths: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn discover_zip(path: &str) -> Result<ZipDiscovery, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let hash = content_hash(&bytes);
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ENTRIES {
        return Err(format!("ZIP exceeds {MAX_ENTRIES} entries"));
    }

    let mut files = Vec::new();
    let mut unknown_paths = Vec::new();
    let mut warnings = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry.name().to_string();
        let relative = normalize_zip_path(&raw_name)?;
        if !relative.to_ascii_lowercase().ends_with(".csv") {
            unknown_paths.push(relative);
            continue;
        }
        if entry.size() > MAX_FILE_BYTES {
            return Err(format!("CSV too large: {relative}"));
        }
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
        let text = text.trim_start_matches('\u{feff}').to_string();
        let headers = parse_headers(&text);
        if is_letterboxd_list_export_path(&relative) {
            unknown_paths.push(relative);
            continue;
        }
        match classify_csv(&relative, &headers) {
            Some(kind) => files.push(ZipCsvFile {
                relative_path: relative,
                kind,
                text,
            }),
            None => {
                unknown_paths.push(relative);
                warnings.push("Unrecognized CSV schema preserved in diagnostics".into());
            }
        }
    }

    if unknown_paths
        .iter()
        .any(|path| is_letterboxd_list_export_path(path))
    {
        warnings.push("Skipped Letterboxd list exports (lists.csv / lists/)".into());
    }

    if files.is_empty() {
        return Err("No recognized Letterboxd CSV datasets found in export".into());
    }

    Ok(ZipDiscovery {
        content_hash: hash,
        files,
        unknown_paths,
        warnings,
    })
}

pub fn normalize_zip_path(raw: &str) -> Result<String, String> {
    let path = Path::new(raw);
    for component in path.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Unsafe ZIP path: {raw}"));
            }
            _ => {}
        }
    }
    let mut parts = Vec::new();
    for component in path.components() {
        if let Component::Normal(part) = component {
            parts.push(part.to_string_lossy().into_owned());
        }
    }
    if parts.is_empty() {
        return Err(format!("Empty ZIP path: {raw}"));
    }
    Ok(parts.join("/"))
}

pub fn discover_zip_bytes(bytes: &[u8]) -> Result<ZipDiscovery, String> {
    let hash = content_hash(bytes);
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ENTRIES {
        return Err(format!("ZIP exceeds {MAX_ENTRIES} entries"));
    }
    let mut files = Vec::new();
    let mut unknown_paths = Vec::new();
    let mut warnings = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let relative = normalize_zip_path(entry.name())?;
        if !relative.to_ascii_lowercase().ends_with(".csv") {
            unknown_paths.push(relative);
            continue;
        }
        if entry.size() > MAX_FILE_BYTES {
            return Err(format!("CSV too large: {relative}"));
        }
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
        let text = text.trim_start_matches('\u{feff}').to_string();
        let headers = parse_headers(&text);
        if is_letterboxd_list_export_path(&relative) {
            unknown_paths.push(relative);
            continue;
        }
        match classify_csv(&relative, &headers) {
            Some(kind) => files.push(ZipCsvFile {
                relative_path: relative,
                kind,
                text,
            }),
            None => unknown_paths.push(relative),
        }
    }
    if unknown_paths
        .iter()
        .any(|path| is_letterboxd_list_export_path(path))
    {
        warnings.push("Skipped Letterboxd list exports (lists.csv / lists/)".into());
    }
    if files.is_empty() {
        return Err("No recognized CSV datasets".into());
    }
    Ok(ZipDiscovery {
        content_hash: hash,
        files,
        unknown_paths,
        warnings,
    })
}

pub fn headers_of(text: &str) -> Vec<String> {
    parse_headers(text)
}
