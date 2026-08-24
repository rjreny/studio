pub fn parse_csv(text: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut lines = text.lines();
    let header_line = match lines.next() {
        Some(line) => line,
        None => return records,
    };
    let headers: Vec<String> = split_csv_line(header_line);
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

pub fn classify_csv(path: &str, headers: &[String]) -> Option<CsvKind> {
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
