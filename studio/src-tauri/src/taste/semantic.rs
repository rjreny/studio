use crate::storage::db::Database;
use crate::taste::features::{keyword_strength, Credit, Keyword, KeywordStrength};
use crate::taste::retrieve::{Candidate, FilmRecord};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const EMBEDDING_MODEL: &str = "qwen/qwen3-embedding-4b";
const EMBEDDING_ENDPOINT: &str = "https://openrouter.ai/api/v1/embeddings";
const EMBEDDING_BATCH_SIZE: usize = 32;
const EMBEDDING_TEXT_LIMIT: usize = 5000;
const TOP_HISTORY_MATCHES: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScore {
    pub positive_similarity: f32,
    pub negative_similarity: f32,
    pub fit: f32,
    pub coverage: bool,
    pub positive_matches: usize,
    pub negative_matches: usize,
}

impl Default for SemanticScore {
    fn default() -> Self {
        Self {
            positive_similarity: 0.0,
            negative_similarity: 0.0,
            fit: 0.5,
            coverage: false,
            positive_matches: 0,
            negative_matches: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStats {
    pub model: String,
    pub rated_items: usize,
    pub candidate_items: usize,
    pub rated_coverage: usize,
    pub candidate_coverage: usize,
    pub cache_hits: usize,
    pub remote_embeddings: usize,
    pub failed_items: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
struct TextItem {
    tmdb_id: i64,
    text: String,
    hash: String,
}

#[derive(Debug, Clone)]
struct RatedVector {
    vector: Vec<f32>,
    positive_weight: f32,
    negative_weight: f32,
}

/// Fetch/cache the small set of vectors needed by one taste run and score every
/// candidate against the strongest positive and negative history matches.
///
/// ponytail: this intentionally uses an in-memory O(candidates * rated-history)
/// cosine scan; the app's local catalog is small enough that a vector database
/// would add more failure modes than value.
pub fn score_candidates(
    db: &Database,
    key: &str,
    films: &[FilmRecord],
    candidates: &[Candidate],
) -> (HashMap<i64, SemanticScore>, SemanticStats) {
    let mut stats = SemanticStats {
        model: EMBEDDING_MODEL.into(),
        rated_items: films.iter().filter(|f| f.rating.is_some()).count(),
        candidate_items: candidates.iter().filter_map(|c| c.tmdb_id).count(),
        ..Default::default()
    };

    let mut history_items = Vec::new();
    let mut positive_weights = HashMap::new();
    let mut negative_weights = HashMap::new();
    let mut seen_history = HashSet::new();
    for film in films.iter().filter(|f| f.rating.is_some()) {
        let Some(id) = film.tmdb_id else { continue };
        if !seen_history.insert(id) {
            continue;
        }
        history_items.push(text_item_for_film(db, film));
        let weight = film
            .signal
            .as_ref()
            .map(|s| s.recommendation_weight.max(0.05))
            .unwrap_or(1.0);
        if film.rating.unwrap_or(3.0) >= 4.0 {
            positive_weights.insert(id, weight);
        } else if film.rating.unwrap_or(3.0) <= 2.5 {
            negative_weights.insert(id, weight);
        }
    }

    let mut candidate_items = Vec::new();
    let mut seen_candidates = HashSet::new();
    for candidate in candidates {
        let Some(id) = candidate.tmdb_id else { continue };
        if seen_candidates.insert(id) {
            candidate_items.push(text_item_for_candidate(db, candidate));
        }
    }

    let all_items = history_items
        .iter()
        .chain(candidate_items.iter())
        .cloned()
        .collect::<Vec<_>>();
    let vectors = load_or_fetch_vectors(db, key, &all_items, &mut stats);

    let mut history_vectors = HashMap::new();
    for item in &history_items {
        if let Some(vector) = vectors.get(&item.tmdb_id) {
            history_vectors.insert(
                item.tmdb_id,
                RatedVector {
                    vector: vector.clone(),
                    positive_weight: positive_weights.get(&item.tmdb_id).copied().unwrap_or(0.0),
                    negative_weight: negative_weights.get(&item.tmdb_id).copied().unwrap_or(0.0),
                },
            );
        }
    }
    stats.rated_coverage = history_vectors.len();

    let mut scores = HashMap::new();
    for item in &candidate_items {
        let Some(vector) = vectors.get(&item.tmdb_id) else {
            continue;
        };
        let score = semantic_score(vector, &history_vectors);
        scores.insert(item.tmdb_id, score);
    }
    stats.candidate_coverage = scores.len();
    (scores, stats)
}

fn semantic_score(candidate: &[f32], history: &HashMap<i64, RatedVector>) -> SemanticScore {
    let mut positives = Vec::new();
    let mut negatives = Vec::new();
    for row in history.values() {
        let similarity = cosine(candidate, &row.vector).max(0.0);
        if row.positive_weight > 0.0 {
            positives.push((similarity, row.positive_weight));
        }
        if row.negative_weight > 0.0 {
            negatives.push((similarity, row.negative_weight));
        }
    }
    let (positive_similarity, positive_matches) = weighted_top_mean(positives);
    let (negative_similarity, negative_matches) = weighted_top_mean(negatives);
    if positive_matches == 0 {
        return SemanticScore::default();
    }
    let margin = (positive_similarity - negative_similarity).clamp(-1.0, 1.0);
    SemanticScore {
        positive_similarity,
        negative_similarity,
        fit: ((margin + 1.0) / 2.0).clamp(0.0, 1.0),
        coverage: true,
        positive_matches,
        negative_matches,
    }
}

fn weighted_top_mean(mut values: Vec<(f32, f32)>) -> (f32, usize) {
    values.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let selected = values.into_iter().take(TOP_HISTORY_MATCHES).collect::<Vec<_>>();
    let weight = selected.iter().map(|(_, w)| *w).sum::<f32>();
    if weight <= 0.0 {
        return (0.0, 0);
    }
    (
        selected.iter().map(|(similarity, w)| similarity * w).sum::<f32>() / weight,
        selected.len(),
    )
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut aa = 0.0;
    let mut bb = 0.0;
    for (left, right) in a.iter().zip(b) {
        dot += left * right;
        aa += left * left;
        bb += right * right;
    }
    let denom = aa.sqrt() * bb.sqrt();
    if denom <= f32::EPSILON {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

fn load_or_fetch_vectors(
    db: &Database,
    key: &str,
    items: &[TextItem],
    stats: &mut SemanticStats,
) -> HashMap<i64, Vec<f32>> {
    let mut vectors = HashMap::new();
    let mut missing = Vec::new();
    for item in items {
        match load_cached_vector(db, item) {
            Ok(Some(vector)) => {
                vectors.insert(item.tmdb_id, vector);
                stats.cache_hits += 1;
            }
            Ok(None) => missing.push(item.clone()),
            Err(err) => {
                missing.push(item.clone());
                stats.error.get_or_insert(err);
            }
        }
    }

    for batch in missing.chunks(EMBEDDING_BATCH_SIZE) {
        let inputs = batch.iter().map(|item| item.text.clone()).collect::<Vec<_>>();
        match request_embeddings_with_retry(key, &inputs) {
            Ok(batch_vectors) if batch_vectors.len() == batch.len() => {
                for (item, vector) in batch.iter().zip(batch_vectors) {
                    if store_cached_vector(db, item, &vector).is_err() {
                        stats.error.get_or_insert("Could not cache semantic embedding".into());
                    }
                    vectors.insert(item.tmdb_id, vector);
                    stats.remote_embeddings += 1;
                }
            }
            Ok(_) => {
                stats.failed_items += batch.len();
                stats.error.get_or_insert("OpenRouter returned an incomplete embedding batch".into());
            }
            Err(err) => {
                stats.failed_items += batch.len();
                stats.error.get_or_insert(err);
            }
        }
    }
    stats.failed_items = stats.failed_items.min(items.len());
    vectors
}

fn request_embeddings_with_retry(key: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
    let mut last_error = None;
    for attempt in 0..3 {
        match request_embeddings(key, inputs) {
            Ok(vectors) => return Ok(vectors),
            Err(err) if attempt < 2 && embedding_error_is_retryable(&err) => {
                last_error = Some(err);
                std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
            }
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| "Embedding request failed".into()))
}

fn embedding_error_is_retryable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("http 429")
        || lower.contains("engine_overloaded")
        || lower.contains("model busy")
        || lower.contains("http 502")
        || lower.contains("http 503")
}

fn request_embeddings(key: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let body = serde_json::json!({
        "model": EMBEDDING_MODEL,
        "input": inputs,
    });
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(90))
        .timeout_write(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build();
    let response = match agent
        .post(EMBEDDING_ENDPOINT)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .set("HTTP-Referer", "https://github.com/rjreny/studio")
        .set("X-Title", "Studio Taste Embeddings")
        .set("User-Agent", "Studio/0.10 (local film app)")
        .send_string(&body.to_string())
    {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            return Err(format!("OpenRouter embeddings HTTP {code}: {}", clip_error(&text)));
        }
        Err(err) => return Err(err.to_string()),
    };
    let value: Value = serde_json::from_str(&response).map_err(|e| e.to_string())?;
    if let Some(error) = value.get("error") {
        return Err(format!("OpenRouter embeddings error: {}", clip_error(&error.to_string())));
    }
    let data = value["data"]
        .as_array()
        .ok_or_else(|| "OpenRouter embeddings response had no data".to_string())?;
    let mut output = vec![None; inputs.len()];
    for row in data {
        let Some(index) = row["index"].as_u64().map(|n| n as usize) else {
            continue;
        };
        if index >= output.len() {
            continue;
        }
        let Some(vector) = row["embedding"].as_array() else {
            continue;
        };
        let parsed = vector
            .iter()
            .filter_map(|v| v.as_f64().map(|n| n as f32))
            .collect::<Vec<_>>();
        if parsed.len() == vector.len() && !parsed.is_empty() && parsed.iter().all(|v| v.is_finite()) {
            output[index] = Some(parsed);
        }
    }
    output
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "OpenRouter embeddings response omitted an item".into())
}

fn load_cached_vector(db: &Database, item: &TextItem) -> Result<Option<Vec<f32>>, String> {
    let row = db
        .conn()
        .query_row(
            "SELECT content_hash, dimension, vector_json FROM taste_embeddings WHERE tmdb_id = ?1 AND model = ?2",
            params![item.tmdb_id, EMBEDDING_MODEL],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((hash, dimension, raw)) = row else {
        return Ok(None);
    };
    if hash != item.hash {
        return Ok(None);
    }
    let vector = serde_json::from_str::<Vec<f32>>(&raw).map_err(|e| e.to_string())?;
    if dimension < 1 || vector.len() != dimension as usize || vector.iter().any(|v| !v.is_finite()) {
        return Ok(None);
    }
    Ok(Some(vector))
}

fn store_cached_vector(db: &Database, item: &TextItem, vector: &[f32]) -> Result<(), String> {
    let raw = serde_json::to_string(vector).map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            r#"INSERT INTO taste_embeddings(tmdb_id, model, content_hash, dimension, vector_json, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT(tmdb_id, model) DO UPDATE SET
                 content_hash = excluded.content_hash,
                 dimension = excluded.dimension,
                 vector_json = excluded.vector_json,
                 updated_at = excluded.updated_at"#,
            params![
                item.tmdb_id,
                EMBEDDING_MODEL,
                item.hash,
                vector.len() as i64,
                raw,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn text_item_for_film(db: &Database, film: &FilmRecord) -> TextItem {
    text_item(
        db,
        film.tmdb_id.unwrap_or(0),
        &film.title,
        film.year,
        &film.genres,
        &film.credits,
        &film.keywords,
    )
}

fn text_item_for_candidate(db: &Database, candidate: &Candidate) -> TextItem {
    text_item(
        db,
        candidate.tmdb_id.unwrap_or(0),
        &candidate.title,
        candidate.year,
        &candidate.genres,
        &candidate.credits,
        &candidate.keywords,
    )
}

fn text_item(
    db: &Database,
    tmdb_id: i64,
    title: &str,
    year: Option<i32>,
    genres: &[String],
    credits: &[Credit],
    keywords: &[Keyword],
) -> TextItem {
    let (overview, tagline) = db
        .conn()
        .query_row(
            "SELECT overview, tagline FROM movies WHERE tmdb_id = ?1 LIMIT 1",
            params![tmdb_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or((None, None));
    let text = canonical_text(title, year, overview.as_deref(), tagline.as_deref(), genres, credits, keywords);
    TextItem {
        tmdb_id,
        hash: text_hash(&text),
        text,
    }
}

pub fn canonical_text(
    title: &str,
    year: Option<i32>,
    overview: Option<&str>,
    tagline: Option<&str>,
    genres: &[String],
    credits: &[Credit],
    keywords: &[Keyword],
) -> String {
    let mut lines = vec![format!(
        "Title: {}{}",
        title.trim(),
        year.map(|y| format!(" ({y})")).unwrap_or_default()
    )];
    if let Some(tagline) = tagline.filter(|v| !v.trim().is_empty()) {
        lines.push(format!("Tagline: {}", clean_text(tagline)));
    }
    if let Some(overview) = overview.filter(|v| !v.trim().is_empty()) {
        lines.push(format!("Overview: {}", clean_text(overview)));
    }
    let genres = genres
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
        .take(8)
        .collect::<Vec<_>>();
    if !genres.is_empty() {
        lines.push(format!("Genres: {}", genres.join(", ")));
    }
    for (label, names) in credit_sections(credits) {
        if !names.is_empty() {
            lines.push(format!("{label}: {}", names.join(", ")));
        }
    }
    let keywords = keywords
        .iter()
        .filter(|k| matches!(keyword_strength(&k.name), KeywordStrength::Strong | KeywordStrength::Thematic))
        .map(|k| k.name.trim())
        .filter(|k| !k.is_empty())
        .take(12)
        .collect::<Vec<_>>();
    if !keywords.is_empty() {
        lines.push(format!("Themes: {}", keywords.join(", ")));
    }
    truncate_chars(&lines.join("\n"), EMBEDDING_TEXT_LIMIT)
}

fn credit_sections(credits: &[Credit]) -> Vec<(&'static str, Vec<String>)> {
    let mut sections: Vec<(&'static str, Vec<String>, usize)> = vec![
        ("Director", Vec::new(), 2),
        ("Writer", Vec::new(), 2),
        ("Cinematographer", Vec::new(), 2),
        ("Composer", Vec::new(), 2),
        ("Actors", Vec::new(), 5),
    ];
    let mut seen = HashSet::new();
    for credit in credits {
        let lower = credit.job.to_ascii_lowercase();
        let section = if lower == "director" {
            Some(0)
        } else if lower.contains("writer") || lower.contains("screenplay") {
            Some(1)
        } else if lower.contains("cinematograph") || lower.contains("photograph") {
            Some(2)
        } else if lower.contains("composer") || lower.contains("music") {
            Some(3)
        } else if lower == "actor" {
            Some(4)
        } else {
            None
        };
        let Some(index) = section else { continue };
        let name = credit.name.trim();
        if name.is_empty() || !seen.insert((index, name.to_ascii_lowercase())) {
            continue;
        }
        if sections[index].1.len() < sections[index].2 {
            sections[index].1.push(name.to_string());
        }
    }
    sections
        .into_iter()
        .map(|(label, names, _)| (label, names))
        .collect()
}

fn clean_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    value.chars().take(limit).collect::<String>()
}

fn text_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn clip_error(value: &str) -> String {
    let flat = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&flat, 300)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credit(job: &str, name: &str) -> Credit {
        Credit {
            id: None,
            name: name.into(),
            job: job.into(),
        }
    }

    #[test]
    fn canonical_text_keeps_meaningful_metadata_and_omits_noisy_keywords() {
        let text = canonical_text(
            "Example",
            Some(2024),
            Some("A detective follows a nonlinear timeline."),
            Some("A short tagline"),
            &["Drama".into()],
            &[credit("Director", "A Director"), credit("Actor", "A Star")],
            &[
                Keyword { id: None, name: "nonlinear".into() },
                Keyword { id: None, name: "woman director".into() },
            ],
        );
        assert!(text.contains("Overview:"));
        assert!(text.contains("Director: A Director"));
        assert!(text.contains("Themes: nonlinear"));
        assert!(!text.contains("woman director"));
    }

    #[test]
    fn cosine_handles_dimension_mismatch() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0]), 0.0);
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn positive_and_negative_matches_form_a_margin() {
        let mut history = HashMap::new();
        history.insert(
            1,
            RatedVector { vector: vec![1.0, 0.0], positive_weight: 1.0, negative_weight: 0.0 },
        );
        history.insert(
            2,
            RatedVector { vector: vec![0.0, 1.0], positive_weight: 0.0, negative_weight: 1.0 },
        );
        let score = semantic_score(&[1.0, 0.0], &history);
        assert!(score.fit > 0.75);
        assert_eq!(score.positive_matches, 1);
        assert_eq!(score.negative_matches, 1);
    }
}
