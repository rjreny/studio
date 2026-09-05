fn strip_bom(s: &str) -> &str {
    s.trim_start_matches('\u{feff}')
}

pub fn parse_headers(text: &str) -> Vec<String> {
    match strip_bom(text).lines().next() {
        Some(line) => split_csv_line(line)
            .into_iter()
            .map(|h| strip_bom(&h).trim().to_string())
            .collect(),
        None => Vec::new(),
    }
}

pub fn parse_csv(text: &str) -> Vec<Record> {
    let text = strip_bom(text);
    let mut records = Vec::new();
    let mut lines = text.lines();
    let header_line = match lines.next() {
        Some(line) => line,
        None => return records,
    };
    let headers: Vec<String> = parse_headers(header_line);
    for (idx, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let values = split_csv_line(line);
        let mut row = Record {
            row_number: (idx + 2) as u32,
            fields: std::collections::HashMap::new(),
        };
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = values.get(i) {
                row.fields.insert(header.clone(), value.clone());
            }
        }
        records.push(row);
    }
    records
}

pub struct Record {
    pub row_number: u32,
    pub fields: std::collections::HashMap<String, String>,
}

impl Record {
    pub fn get(&self, keys: &[&str]) -> String {
        for key in keys {
            if let Some(value) = self.fields.get(*key) {
                if !value.trim().is_empty() {
                    return value.trim().to_string();
                }
            }
            let lower = key.to_lowercase();
            for (k, v) in &self.fields {
                if k.to_lowercase() == lower && !v.trim().is_empty() {
                    return v.trim().to_string();
                }
            }
        }
        String::new()
    }
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    out.push(current.trim().to_string());
    out
}

/// Letterboxd data exports include `lists.csv` (an index of list titles) and
/// per-list CSVs under `lists/`. Those are not films. `lists.csv` has Date+Name
/// columns, so without this guard it was misclassified as diary and list titles
/// landed in the Films library.
pub fn is_letterboxd_list_export_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let file_name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if file_name == "lists.csv" {
        return true;
    }
    lower.starts_with("lists/") || lower.contains("/lists/")
}

pub fn classify_csv(path: &str, headers: &[String]) -> Option<CsvKind> {
    if is_letterboxd_list_export_path(path) {
        return None;
    }

    let lower = path.to_ascii_lowercase();
    let joined = headers
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let has = |name: &str| joined.iter().any(|h| h == name);

    if lower.contains("diary") && has("name") && (has("watched date") || has("date")) {
        return Some(CsvKind::Diary);
    }
    if lower.contains("rating") && has("name") && has("rating") {
        return Some(CsvKind::Ratings);
    }
    if lower.contains("watchlist") && has("name") {
        return Some(CsvKind::Watchlist);
    }
    if lower.contains("watched") && has("name") && !has("watched date") {
        return Some(CsvKind::Watched);
    }
    if lower.contains("review") && has("name") {
        return Some(CsvKind::Reviews);
    }
    if has("name") && has("rating") {
        return Some(CsvKind::Ratings);
    }
    if has("name") && (has("watched date") || has("date")) {
        return Some(CsvKind::Diary);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvKind {
    Diary,
    Ratings,
    Watched,
    Watchlist,
    Reviews,
}

impl CsvKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diary => "diary",
            Self::Ratings => "ratings",
            Self::Watched => "watched",
            Self::Watchlist => "watchlist",
            Self::Reviews => "reviews",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utf8_bom_from_headers() {
        let text = "\u{feff}Date,Name,Year\n2020-01-01,Inception,2010\n";
        let rows = parse_csv(text);
        assert_eq!(parse_headers(text), vec!["Date", "Name", "Year"]);
        assert_eq!(rows[0].get(&["Name"]), "Inception");
        assert_eq!(
            classify_csv("diary.csv", &parse_headers(text)),
            Some(CsvKind::Diary)
        );
    }

    #[test]
    fn list_export_paths_are_never_classified_as_films() {
        let list_index = parse_headers("Date,Name,Tags,URL,Description");
        let list_films = parse_headers("Position,Name,Year,Letterboxd URI,Description");
        assert!(is_letterboxd_list_export_path("lists.csv"));
        assert!(is_letterboxd_list_export_path("export/lists.csv"));
        assert!(is_letterboxd_list_export_path("lists/favorites.csv"));
        assert!(is_letterboxd_list_export_path("backup/lists/2024.csv"));
        assert!(!is_letterboxd_list_export_path("watchlist.csv"));
        assert!(!is_letterboxd_list_export_path("diary.csv"));
        assert_eq!(classify_csv("lists.csv", &list_index), None);
        assert_eq!(classify_csv("lists/favorites.csv", &list_films), None);
        assert_eq!(
            classify_csv("lists/films-i-watched.csv", &list_films),
            None
        );
    }
}
