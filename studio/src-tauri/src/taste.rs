use crate::catalog::tmdb;
use crate::letterboxd::posters::poster_url;
use crate::letterboxd::rss::parse_activity_payload;
use crate::models::{JobProgress, LibraryItem};
use crate::storage::db::Database;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const KEYRING_SERVICE: &str = "studio";
const KEYRING_USER: &str = "openrouter_api_key";
const META_REPORT: &str = "taste_report";
const META_MODEL: &str = "taste_model";
const META_WEB: &str = "taste_web";
const OPENROUTER_CHAT: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_KEY: &str = "https://openrouter.ai/api/v1/key";

const MODEL_QWEN: &str = "qwen";
const MODEL_GEMINI: &str = "gemini";
const MODEL_DEEPSEEK: &str = "deepseek";
const MODEL_KIMI: &str = "kimi-k3";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteModelInfo {
    pub id: String,
    pub label: String,
    pub blurb: String,
    pub context: String,
    pub cost: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteKeyStatus {
    pub stored: bool,
    pub valid: Option<bool>,
    pub last_error: Option<String>,
    pub model: String,
    #[serde(default)]
    pub web: bool,
    #[serde(default)]
    pub models: Vec<TasteModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteAffinity {
    pub label: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteDimension {
    pub name: String,
    pub take: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TastePick {
    pub title: String,
    pub year: Option<i32>,
    pub poster: Option<String>,
    pub why: String,
    pub rhymes_with: Vec<String>,
    pub film_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteStat {
    pub label: String,
    pub count: u32,
    pub avg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteSnapshot {
    pub rated_count: u32,
    pub loved_count: u32,
    pub hated_count: u32,
    pub avg_rating: Option<f64>,
    pub genres: Vec<TasteStat>,
    pub decades: Vec<TasteStat>,
    pub directors: Vec<TasteStat>,
    pub actors: Vec<TasteStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteReport {
    pub title: String,
    pub summary: String,
    pub affinities: Vec<TasteAffinity>,
    pub aversions: Vec<TasteAffinity>,
    pub dimensions: Vec<TasteDimension>,
    pub picks: Vec<TastePick>,
    pub model: String,
    pub generated_at: String,
    pub rated_count: u32,
    #[serde(default)]
    pub web_used: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteState {
    pub key: TasteKeyStatus,
    pub snapshot: TasteSnapshot,
    pub report: Option<TasteReport>,
}

#[derive(Clone)]
struct FilmRow {
    key: String,
    title: String,
    year: Option<i32>,
    rating: Option<f64>,
    liked: bool,
    watched: bool,
    watchlist: bool,
    viewings: u32,
    genres: Vec<String>,
    cast: Vec<String>,
    directors: Vec<String>,
    writers: Vec<String>,
    cinematographers: Vec<String>,
    composers: Vec<String>,
    overview: Option<String>,
    runtime: Option<i32>,
    tagline: Option<String>,
    similar: Vec<LibraryItem>,
    poster: Option<String>,
}

struct Budget {
    really_liked: usize,
    liked: usize,
    hated: usize,
    candidates: usize,
    watchlist: usize,
    overview_chars: usize,
    max_chars: usize,
}

fn budget_for(model: &str) -> Budget {
    if model == MODEL_KIMI {
        Budget {
            really_liked: 220,
            liked: 80,
            hated: 80,
            candidates: 64,
            watchlist: 20,
            overview_chars: 140,
            max_chars: 240_000,
        }
    } else {
        Budget {
            really_liked: 180,
            liked: 70,
            hated: 70,
            candidates: 56,
            watchlist: 18,
            overview_chars: 120,
            max_chars: 200_000,
        }
    }
}

fn estimate_tokens(payload: &Value) -> usize {
    (SYSTEM_PROMPT.len() + payload.to_string().len()) / 4 + 48
}

struct Corpus {
    snapshot: TasteSnapshot,
    payload: Value,
    seen: HashSet<String>,
}

pub fn default_model() -> String {
    MODEL_QWEN.to_string()
}

pub fn normalize_model(raw: &str) -> String {
    let compact = raw
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_'], "-");
    if compact.contains("kimi") || compact == "k3" {
        MODEL_KIMI.into()
    } else if compact.contains("gemini") {
        MODEL_GEMINI.into()
    } else if compact.contains("deepseek") {
        MODEL_DEEPSEEK.into()
    } else if compact.contains("qwen") || compact.is_empty() {
        MODEL_QWEN.into()
    } else if compact == MODEL_GEMINI || compact == MODEL_DEEPSEEK || compact == MODEL_KIMI {
        compact
    } else {
        MODEL_QWEN.into()
    }
}

fn openrouter_model_id(model: &str) -> &'static str {
    match model {
        MODEL_KIMI => "moonshotai/kimi-k3",
        MODEL_GEMINI => "google/gemini-3.7-flash",
        MODEL_DEEPSEEK => "deepseek/deepseek-v4-flash",
        _ => "qwen/qwen3.7-flash",
    }
}

fn model_label(model: &str) -> &'static str {
    match model {
        MODEL_KIMI => "Kimi K3",
        MODEL_GEMINI => "Gemini Flash",
        MODEL_DEEPSEEK => "DeepSeek V4 Flash",
        _ => "Qwen Flash",
    }
}

pub fn model_catalog() -> Vec<TasteModelInfo> {
    vec![
        TasteModelInfo {
            id: MODEL_QWEN.into(),
            label: "Qwen Flash".into(),
            blurb: "Cheapest 1M-context reader. Best stretch of $10.".into(),
            context: "1M".into(),
            cost: "cheapest".into(),
        },
        TasteModelInfo {
            id: MODEL_GEMINI.into(),
            label: "Gemini Flash".into(),
            blurb: "Stronger reasoning, still cheap, same huge window.".into(),
            context: "1M".into(),
            cost: "cheap".into(),
        },
        TasteModelInfo {
            id: MODEL_DEEPSEEK.into(),
            label: "DeepSeek V4 Flash".into(),
            blurb: "Fast 1M reader if you already like DeepSeek.".into(),
            context: "1M".into(),
            cost: "cheap".into(),
        },
        TasteModelInfo {
            id: MODEL_KIMI.into(),
            label: "Kimi K3".into(),
            blurb: "Sharpest take. Slow and expensive. Several minutes is normal.".into(),
            context: "1M".into(),
            cost: "expensive".into(),
        },
    ]
}

fn request_timeout(model: &str) -> Duration {
    match model {
        MODEL_KIMI => Duration::from_secs(360),
        MODEL_GEMINI => Duration::from_secs(180),
        _ => Duration::from_secs(120),
    }
}

fn empty_status() -> TasteKeyStatus {
    TasteKeyStatus {
        stored: false,
        valid: None,
        last_error: None,
        model: default_model(),
        web: true,
        models: Vec::new(),
    }
}

fn with_prefs(db: &Database, mut status: TasteKeyStatus) -> Result<TasteKeyStatus, String> {
    status.model = stored_model(db)?;
    status.web = stored_web(db)?;
    status.models = model_catalog();
    Ok(status)
}

pub fn stored_model(db: &Database) -> Result<String, String> {
    Ok(db
        .get_meta(META_MODEL)?
        .map(|m| normalize_model(&m))
        .unwrap_or_else(default_model))
}

pub fn set_model(db: &Database, model: &str) -> Result<String, String> {
    let id = normalize_model(model);
    db.set_meta(META_MODEL, &id)?;
    Ok(id)
}

pub fn stored_web(db: &Database) -> Result<bool, String> {
    Ok(match db.get_meta(META_WEB)?.as_deref() {
        Some("0") | Some("false") => false,
        _ => true,
    })
}

pub fn set_web(db: &Database, enabled: bool) -> Result<bool, String> {
    db.set_meta(META_WEB, if enabled { "1" } else { "0" })?;
    Ok(enabled)
}

pub fn get_api_key() -> Result<Option<String>, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| e.to_string())?
        .get_password()
        .map(|k| {
            let trimmed = k.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .or_else(|e| {
            if e.to_string().contains("No matching entry") {
                Ok(None)
            } else {
                Err(e.to_string())
            }
        })
}

fn set_api_key(key: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| e.to_string())?
        .set_password(key)
        .map_err(|e| e.to_string())
}

pub fn clear_api_key() -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| e.to_string())?
        .set_password("")
        .map_err(|e| e.to_string())
}

fn probe_key(key: &str) -> Result<TasteKeyStatus, String> {
    let req = ureq::get(OPENROUTER_KEY)
        .set("Authorization", &format!("Bearer {key}"))
        .set("User-Agent", "Studio/0.6 (local film app)")
        .timeout(Duration::from_secs(12));
    match req.call() {
        Ok(_) => Ok(TasteKeyStatus {
            stored: false,
            valid: Some(true),
            last_error: None,
            model: default_model(),
            web: true,
            models: Vec::new(),
        }),
        Err(err) => {
            let text = err.to_string();
            let invalid = text.contains("401") || text.contains("403");
            Ok(TasteKeyStatus {
                stored: false,
                valid: if invalid { Some(false) } else { None },
                last_error: Some(if invalid {
                    "OpenRouter rejected this key. Create a pay-as-you-go key at openrouter.ai/keys."
                        .into()
                } else {
                    format!("Could not reach OpenRouter: {text}")
                }),
                model: default_model(),
                web: true,
                models: Vec::new(),
            })
        }
    }
}

pub fn stored_status(db: &Database) -> Result<TasteKeyStatus, String> {
    with_prefs(
        db,
        TasteKeyStatus {
            stored: get_api_key()?.is_some(),
            valid: None,
            last_error: None,
            model: default_model(),
            web: true,
            models: Vec::new(),
        },
    )
}

pub fn key_status(db: &Database) -> Result<TasteKeyStatus, String> {
    match get_api_key()? {
        Some(key) => {
            let mut status = probe_key(&key)?;
            status.stored = true;
            with_prefs(db, status)
        }
        None => with_prefs(db, empty_status()),
    }
}

pub fn store_api_key(db: &Database, key: &str) -> Result<TasteKeyStatus, String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("Paste an OpenRouter API key first".into());
    }
    let mut status = probe_key(trimmed)?;
    status = with_prefs(db, status)?;
    if status.valid != Some(true) {
        let previous = get_api_key()?.is_some();
        status.stored = previous;
        return Ok(status);
    }
    set_api_key(trimmed)?;
    status.stored = true;
    Ok(status)
}

pub fn load_state(db: &Database) -> Result<TasteState, String> {
    let films = collect_films(db)?;
    let report = db
        .get_meta(META_REPORT)?
        .and_then(|raw| serde_json::from_str(&raw).ok());
    Ok(TasteState {
        key: stored_status(db)?,
        snapshot: snapshot_of(&films),
        report,
    })
}

pub fn analyze(
    db: &Database,
    progress: &mut dyn FnMut(JobProgress),
) -> Result<TasteReport, String> {
    let key = get_api_key()?.ok_or_else(|| {
        "Add an OpenRouter key in Settings. Qwen Flash is the cheap default."
            .to_string()
    })?;
    let model = stored_model(db)?;
    let web = stored_web(db)?;
    progress(JobProgress {
        job: "taste".into(),
        label: "Reading your log…".into(),
        current: 1,
        total: 3,
        ..Default::default()
    });
    let corpus = build_corpus(db, &model)?;
    if corpus.snapshot.rated_count < 8 {
        return Err("Rate at least 8 films first so the agent has edges to work with.".into());
    }
    let ask_label = if web {
        format!(
            "Asking {} to read your taste, with a few web searches…",
            model_label(&model)
        )
    } else {
        format!("Asking {} to read your taste…", model_label(&model))
    };
    progress(JobProgress {
        job: "taste".into(),
        label: ask_label,
        current: 2,
        total: 3,
        ..Default::default()
    });
    let mut used_model = model.clone();
    let mut used_web = web;
    let mut note = None;
    let raw = match chat_complete(&key, &model, &corpus.payload, web) {
        Ok(raw) => raw,
        Err(err) if should_fallback(&model, &err) => {
            progress(JobProgress {
                job: "taste".into(),
                label: format!(
                    "{} stalled. Retrying with Qwen Flash, no web search…",
                    model_label(&model)
                ),
                current: 2,
                total: 3,
                ..Default::default()
            });
            used_model = MODEL_QWEN.to_string();
            used_web = false;
            note = Some(format!(
                "Fell back to Qwen Flash after {} timed out or failed.",
                model_label(&model)
            ));
            chat_complete(&key, MODEL_QWEN, &corpus.payload, false)
                .map_err(|e| friendly_err(&e, MODEL_QWEN))?
        }
        Err(err) => return Err(friendly_err(&err, &model)),
    };
    let parsed = parse_model_report(&raw)?;
    progress(JobProgress {
        job: "taste".into(),
        label: "Matching posters…".into(),
        current: 3,
        total: 3,
        ..Default::default()
    });
    let picks = hydrate_picks(db, parsed.picks, &corpus.seen)?;
    let report = TasteReport {
        title: parsed.title,
        summary: parsed.summary,
        affinities: parsed.affinities,
        aversions: parsed.aversions,
        dimensions: parsed.dimensions,
        picks,
        model: model_label(&used_model).into(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        rated_count: corpus.snapshot.rated_count,
        web_used: used_web,
        note,
    };
    db.set_meta(META_REPORT, &serde_json::to_string(&report).unwrap_or_default())?;
    Ok(report)
}

fn chat_complete(key: &str, model: &str, payload: &Value, web: bool) -> Result<String, String> {
    match send_chat(key, model, payload, web) {
        Ok(raw) => Ok(raw),
        Err(err) if web && tools_unsupported(&err) => send_chat(key, model, payload, false),
        Err(err) => Err(err),
    }
}

fn send_chat(key: &str, model: &str, payload: &Value, web: bool) -> Result<String, String> {
    let timeout = request_timeout(model);
    let system = if web {
        format!("{SYSTEM_PROMPT}\n{SYSTEM_WEB}")
    } else {
        SYSTEM_PROMPT.to_string()
    };
    let mut body = json!({
        "model": openrouter_model_id(model),
        "temperature": 0.35,
        "max_tokens": 2200,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": payload.to_string() }
        ]
    });
    if web {
        body["tools"] = json!([{
            "type": "openrouter:web_search",
            "parameters": {
                "max_results": 4,
                "max_total_results": 8,
                "max_uses": 3,
                "search_context_size": "low"
            }
        }]);
    } else {
        body["response_format"] = json!({ "type": "json_object" });
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(timeout)
        .timeout_write(Duration::from_secs(30))
        .timeout(timeout + Duration::from_secs(20))
        .build();
    let response = match agent
        .post(OPENROUTER_CHAT)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .set("HTTP-Referer", "https://github.com/rjreny/studio")
        .set("X-Title", "Studio Taste")
        .set("User-Agent", "Studio/0.6 (local film app)")
        .send_string(&body.to_string())
    {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(_, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                if let Some(msg) = v["error"]["message"].as_str() {
                    return Err(msg.to_string());
                }
            }
            return Err(if text.is_empty() {
                "OpenRouter returned an error status".into()
            } else {
                text
            });
        }
        Err(err) => return Err(err.to_string()),
    };
    let v: Value = serde_json::from_str(&response).map_err(|e| {
        format!("OpenRouter returned non-JSON: {e}")
    })?;
    if let Some(err) = v["error"]["message"].as_str() {
        return Err(err.to_string());
    }
    let content = v["choices"]
        .as_array()
        .and_then(|c| c.first())
        .and_then(choice_text)
        .ok_or_else(|| "OpenRouter response had no text".to_string())?;
    Ok(content)
}

fn choice_text(choice: &Value) -> Option<String> {
    let msg = &choice["message"];
    if let Some(s) = msg["content"].as_str().filter(|s| !s.trim().is_empty()) {
        return Some(s.to_string());
    }
    if let Some(parts) = msg["content"].as_array() {
        let joined: String = parts
            .iter()
            .filter_map(|part| part["text"].as_str().or_else(|| part.as_str()))
            .collect::<Vec<_>>()
            .join("");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    msg["reasoning"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .or_else(|| choice["text"].as_str().map(|s| s.to_string()))
}

fn is_timeout_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("10060")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("time out")
}

fn tools_unsupported(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("does not support")
        || lower.contains("tool use")
        || lower.contains("tools are not")
        || lower.contains("unknown tool")
}

fn should_fallback(model: &str, err: &str) -> bool {
    if model == MODEL_QWEN {
        return false;
    }
    let lower = err.to_ascii_lowercase();
    is_timeout_error(err)
        || lower.contains("10060")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("524")
        || lower.contains("no endpoints")
        || lower.contains("connection reset")
        || lower.contains("failed to connect")
}

fn friendly_err(err: &str, model: &str) -> String {
    if is_timeout_error(err) {
        return format!(
            "{} did not finish in time. Flash models usually return in under a minute. Kimi K3 can take several minutes. Taste will retry with Qwen Flash when it can; otherwise pick a Flash model and try again.",
            model_label(model)
        );
    }
    err.to_string()
}

const SYSTEM_WEB: &str = r#"Web search is available. Use at most three cheap searches for critic lists or "films like" a title they loved. Prefer Letterboxd, Criterion, Sight and Sound, and BFI. Use the web to confirm a fit, not to dump random new releases. Never recommend a film from reallyLiked, liked, or disliked."#;

const SYSTEM_PROMPT: &str = r#"You are a film taste analyst inside Studio, a local Letterboxd library app.
You receive compact ratings, catalog metadata, statistical affinities, and a candidate pool. Studio already excluded logged films from candidates and will drop any wildcard that is already in the library.

Return JSON only:
{
  "title": "short taste type, 2-5 words",
  "summary": "2-3 sentences on WHY they like and dislike. Cite specific films.",
  "affinities": [{"label":"...", "evidence":"..."}],
  "aversions": [{"label":"...","evidence":"..."}],
  "dimensions": [
    {"name":"genre","take":"..."},
    {"name":"era","take":"..."},
    {"name":"director","take":"..."},
    {"name":"performance","take":"..."},
    {"name":"image","take":"..."},
    {"name":"intensity","take":"..."},
    {"name":"motif","take":"..."}
  ],
  "picks": [
    {"title":"Exact film title","year":1999,"why":"specific reason tied to their log","rhymesWith":["Film they loved"],"fromPool":true}
  ]
}

Rules:
- Use ratings, hearts, genres, decades, directors, actors, cinematographers, writers, composers, runtime, overviews, friend love, watchlist.
- Contrast loved vs hated. Do not flatten them into "you like good movies".
- Prefer ranking `candidates`. You may add up to 4 real wildcard films not in the lists if they clearly fit.
- Never recommend a title that appears in reallyLiked, liked, or disliked.
- 12 picks. Each why must name at least one film they rated.
- Be concrete. No marketing language. No em dashes.
"#;

#[derive(Deserialize)]
struct ModelReport {
    title: String,
    summary: String,
    #[serde(default)]
    affinities: Vec<TasteAffinity>,
    #[serde(default)]
    aversions: Vec<TasteAffinity>,
    #[serde(default)]
    dimensions: Vec<TasteDimension>,
    #[serde(default)]
    picks: Vec<ModelPick>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelPick {
    title: String,
    year: Option<i32>,
    #[serde(default)]
    why: String,
    #[serde(default)]
    rhymes_with: Vec<String>,
    #[serde(default)]
    from_pool: bool,
}

fn parse_model_report(raw: &str) -> Result<ModelReport, String> {
    let value = extract_json(raw)?;
    serde_json::from_value(value).map_err(|e| format!("Could not parse taste JSON: {e}"))
}

pub fn extract_json(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    let unfenced = if let Some(rest) = trimmed.strip_prefix("```json") {
        rest.trim_start().trim_end_matches('`').trim()
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.trim_start().trim_end_matches('`').trim()
    } else {
        trimmed
    };
    if let Ok(v) = serde_json::from_str::<Value>(unfenced) {
        return Ok(v);
    }
    let start = unfenced.find('{').ok_or("Model did not return JSON")?;
    let end = unfenced.rfind('}').ok_or("Model did not return JSON")?;
    serde_json::from_str(&unfenced[start..=end]).map_err(|e| e.to_string())
}

fn hydrate_picks(
    db: &Database,
    picks: Vec<ModelPick>,
    seen: &HashSet<String>,
) -> Result<Vec<TastePick>, String> {
    let mut out = Vec::new();
    for pick in picks.into_iter().take(16) {
        let title = pick.title.trim();
        if title.is_empty() {
            continue;
        }
        if seen.contains(&seen_key(title, pick.year)) {
            continue;
        }
        let mut year = pick.year;
        let mut film_id = library_id_for(db, title, pick.year)?;
        let mut poster = None;
        let mut tmdb_id = None;
        if let Some(id) = film_id.as_deref() {
            if let Ok((p, t)) = poster_and_tmdb_for(db, id) {
                poster = p;
                tmdb_id = t;
            }
        }
        if poster.is_none() || tmdb_id.is_none() {
            if let Ok(Some(hit)) = tmdb::lookup_movie(title, pick.year) {
                if tmdb_id.is_none() {
                    tmdb_id = Some(hit.tmdb_id);
                }
                if poster.is_none() {
                    poster = hit.poster;
                }
                if pick.year.is_none() {
                    year = hit.year;
                }
                if film_id.is_none() {
                    film_id = library_id_for_tmdb(db, hit.tmdb_id)?
                        .or_else(|| Some(format!("tmdb:{}", hit.tmdb_id)));
                }
            }
        }
        if film_id.is_none() {
            if let Some(id) = tmdb_id {
                film_id = Some(format!("tmdb:{id}"));
            }
        }
        if seen.contains(&seen_key(title, year)) {
            continue;
        }
        out.push(TastePick {
            title: title.to_string(),
            year,
            poster,
            why: pick.why,
            rhymes_with: pick.rhymes_with,
            film_id,
            tmdb_id,
            source: if pick.from_pool { "pool".into() } else { "wildcard".into() },
        });
    }
    Ok(out)
}

fn library_id_for(db: &Database, title: &str, year: Option<i32>) -> Result<Option<String>, String> {
    let needle = title.trim().to_lowercase();
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), ''))
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            WHERE lower(COALESCE(m.canonical_title, json_extract(smr.raw_identity, '$.title'), smr.normalized_title)) = ?1
              AND (?2 IS NULL OR COALESCE(m.release_year, smr.release_year) = ?2)
            LIMIT 1
            "#,
        )
        .map_err(|e| e.to_string())?;
    stmt.query_row(params![needle, year], |row| row.get(0))
        .optional_row()
}

fn library_id_for_tmdb(db: &Database, tmdb_id: i64) -> Result<Option<String>, String> {
    db.conn()
        .query_row(
            r#"
            SELECT COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), ''))
            FROM movies m
            JOIN movie_links ml ON ml.movie_id = m.id
            JOIN source_movie_records smr ON smr.id = ml.source_movie_record_id
            WHERE m.tmdb_id = ?1
            LIMIT 1
            "#,
            params![tmdb_id],
            |row| row.get(0),
        )
        .optional_row()
}

fn poster_and_tmdb_for(db: &Database, id: &str) -> Result<(Option<String>, Option<i64>), String> {
    db.conn()
        .query_row(
            r#"
            SELECT COALESCE(m.poster_path, smr.cached_poster_url), m.tmdb_id
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            WHERE smr.id = ?1
               OR ml.movie_id = ?1
               OR (smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), '')) = ?1
               OR ('tmdb:' || CAST(m.tmdb_id AS TEXT)) = ?1
            LIMIT 1
            "#,
            params![id],
            |row| {
                let path: Option<String> = row.get(0)?;
                Ok((poster_url(path), row.get(1)?))
            },
        )
        .optional_row()
        .map(|v| v.unwrap_or((None, None)))
}

trait OptionalRow<T> {
    fn optional_row(self) -> Result<Option<T>, String>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional_row(self) -> Result<Option<T>, String> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn collect_films(db: &Database) -> Result<Vec<FilmRow>, String> {
    let films = load_films(db)?;
    let mut by_key: HashMap<String, FilmRow> = HashMap::new();
    for film in films {
        by_key
            .entry(film.key.clone())
            .and_modify(|existing| {
                if existing.rating.is_none() {
                    existing.rating = film.rating;
                }
                existing.liked |= film.liked;
                existing.watched |= film.watched;
                existing.watchlist |= film.watchlist;
                existing.viewings += film.viewings;
                if existing.genres.is_empty() {
                    existing.genres = film.genres.clone();
                }
                if existing.directors.is_empty() {
                    existing.directors = film.directors.clone();
                }
                if existing.poster.is_none() {
                    existing.poster = film.poster.clone();
                }
            })
            .or_insert(film);
    }
    Ok(by_key.into_values().collect())
}

fn snapshot_of(films: &[FilmRow]) -> TasteSnapshot {
    let rated: Vec<&FilmRow> = films.iter().filter(|f| f.rating.is_some()).collect();
    let loved = rated
        .iter()
        .filter(|f| f.rating.unwrap_or(0.0) >= 4.5)
        .count() as u32;
    let hated = rated
        .iter()
        .filter(|f| f.rating.unwrap_or(0.0) <= 2.5)
        .count() as u32;
    let avg = if rated.is_empty() {
        None
    } else {
        Some(
            (rated.iter().map(|f| f.rating.unwrap_or(0.0)).sum::<f64>() / rated.len() as f64
                * 100.0)
                .round()
                / 100.0,
        )
    };
    TasteSnapshot {
        rated_count: rated.len() as u32,
        loved_count: loved,
        hated_count: hated,
        avg_rating: avg,
        genres: person_stats(films, |f| f.genres.clone(), 16),
        decades: decade_stats(films),
        directors: person_stats(films, |f| f.directors.clone(), 20),
        actors: person_stats(films, |f| actor_names(&f.cast), 18),
    }
}

fn build_corpus(db: &Database, model: &str) -> Result<Corpus, String> {
    let films = collect_films(db)?;
    let snapshot = snapshot_of(&films);
    let seen: HashSet<String> = films
        .iter()
        .filter(|f| f.watched || f.rating.is_some())
        .map(|f| seen_key(&f.title, f.year))
        .collect();
    let payload = encode_payload(db, &films, &snapshot, &seen, model)?;
    Ok(Corpus {
        snapshot,
        payload,
        seen,
    })
}

fn encode_payload(
    db: &Database,
    films: &[FilmRow],
    snapshot: &TasteSnapshot,
    seen: &HashSet<String>,
    model: &str,
) -> Result<Value, String> {
    let mut budget = budget_for(model);
    let rated: Vec<&FilmRow> = films.iter().filter(|f| f.rating.is_some()).collect();
    let mut liked_sorted: Vec<&FilmRow> = rated
        .iter()
        .copied()
        .filter(|f| f.rating.unwrap_or(0.0) >= 4.0)
        .collect();
    liked_sorted.sort_by(|a, b| {
        b.rating
            .partial_cmp(&a.rating)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut hated_sorted: Vec<&FilmRow> = rated
        .iter()
        .copied()
        .filter(|f| f.rating.unwrap_or(0.0) <= 2.5)
        .collect();
    hated_sorted.sort_by(|a, b| {
        a.rating
            .partial_cmp(&b.rating)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let candidates = candidate_pool(films, &liked_sorted, seen, db, budget.candidates)?;
    let dps = person_stats(films, |f| f.cinematographers.clone(), 12);
    let writers = person_stats(films, |f| f.writers.clone(), 12);
    let composers = person_stats(films, |f| f.composers.clone(), 8);
    let runtimes = runtime_stats(&rated);

    for _ in 0..6 {
        let really_liked: Vec<Value> = liked_sorted
            .iter()
            .filter(|f| f.rating.unwrap_or(0.0) >= 4.5)
            .take(budget.really_liked)
            .map(|f| compact_film(f, budget.overview_chars))
            .collect();
        let liked: Vec<Value> = liked_sorted
            .iter()
            .filter(|f| f.rating.unwrap_or(0.0) < 4.5)
            .take(budget.liked)
            .map(|f| compact_film(f, 0))
            .collect();
        let disliked: Vec<Value> = hated_sorted
            .iter()
            .take(budget.hated)
            .map(|f| compact_film(f, budget.overview_chars))
            .collect();
        let watchlist: Vec<Value> = films
            .iter()
            .filter(|f| f.watchlist && !f.watched)
            .take(budget.watchlist)
            .map(|f| compact_film(f, 0))
            .collect();
        let payload = json!({
            "stats": {
                "rated": snapshot.rated_count,
                "loved45plus": snapshot.loved_count,
                "hated": snapshot.hated_count,
                "avg": snapshot.avg_rating,
                "genres": stats_json(&snapshot.genres),
                "decades": stats_json(&snapshot.decades),
                "directors": stats_json(&snapshot.directors),
                "actors": stats_json(&snapshot.actors),
                "cinematographers": stats_json(&dps),
                "writers": stats_json(&writers),
                "composers": stats_json(&composers),
                "runtime": runtimes,
            },
            "reallyLiked": really_liked,
            "liked": liked,
            "disliked": disliked,
            "watchlist": watchlist,
            "candidates": candidates.iter().take(budget.candidates).cloned().collect::<Vec<_>>(),
        });
        if payload.to_string().len() <= budget.max_chars {
            return Ok(payload);
        }
        budget.overview_chars /= 2;
        budget.really_liked = (budget.really_liked / 2).max(24);
        budget.liked = (budget.liked / 2).max(12);
        budget.hated = (budget.hated / 2).max(12);
        budget.candidates = (budget.candidates / 2).max(16);
        budget.watchlist = (budget.watchlist / 2).max(4);
    }
    Err("Taste log is still too large after shrinking. Wait until posters finish matching, then try again.".into())
}

fn stats_json(stats: &[TasteStat]) -> Vec<Value> {
    stats
        .iter()
        .map(|s| json!({"n": s.label, "c": s.count, "avg": s.avg}))
        .collect()
}

fn compact_film(film: &FilmRow, overview_chars: usize) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("t".into(), json!(film.title));
    if let Some(year) = film.year {
        map.insert("y".into(), json!(year));
    }
    if let Some(rating) = film.rating {
        map.insert("r".into(), json!(rating));
    }
    if !film.genres.is_empty() {
        map.insert("g".into(), json!(film.genres));
    }
    let directors: Vec<String> = film.directors.iter().take(2).cloned().collect();
    if !directors.is_empty() {
        map.insert("d".into(), json!(directors));
    }
    let cast: Vec<String> = actor_names(&film.cast).into_iter().take(3).collect();
    if !cast.is_empty() {
        map.insert("c".into(), json!(cast));
    }
    if let Some(dp) = film.cinematographers.first() {
        map.insert("dp".into(), json!(dp));
    }
    let writers: Vec<String> = film.writers.iter().take(2).cloned().collect();
    if !writers.is_empty() {
        map.insert("w".into(), json!(writers));
    }
    if let Some(runtime) = film.runtime {
        map.insert("rt".into(), json!(runtime));
    }
    if film.liked {
        map.insert("heart".into(), json!(true));
    }
    if overview_chars > 0 {
        if let Some(tag) = film.tagline.as_deref().filter(|s| !s.is_empty()) {
            map.insert("tag".into(), json!(clip(tag, 72)));
        }
        if let Some(ov) = film.overview.as_deref().filter(|s| !s.is_empty()) {
            map.insert("ov".into(), json!(clip(ov, overview_chars)));
        }
    }
    Value::Object(map)
}

fn candidate_pool(
    films: &[FilmRow],
    liked: &[&FilmRow],
    seen: &HashSet<String>,
    db: &Database,
    max: usize,
) -> Result<Vec<Value>, String> {
    let mut out: Vec<Value> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    let similar_cap = max.min(40);
    for seed in liked.iter().take(12) {
        for item in seed.similar.iter().take(4) {
            let key = seen_key(&item.title, item.year);
            if seen.contains(&key) || !used.insert(key) {
                continue;
            }
            out.push(json!({
                "t": item.title,
                "y": item.year,
                "src": format!("similar to {}", seed.title),
            }));
            if out.len() >= similar_cap {
                break;
            }
        }
        if out.len() >= similar_cap {
            break;
        }
    }

    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT f.username, fa.raw_payload, fa.rating
            FROM friend_activity fa
            JOIN friends f ON f.id = fa.friend_id
            WHERE fa.rating >= 4
            ORDER BY fa.rating DESC
            LIMIT 80
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<f64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows.flatten() {
        let (who, raw, rating) = row;
        let (title, year) = parse_activity_payload(&raw);
        let key = seen_key(&title, year);
        if seen.contains(&key) || !used.insert(key) {
            continue;
        }
        out.push(json!({
            "t": title,
            "y": year,
            "src": format!("@{who} {}", rating.unwrap_or(0.0)),
        }));
        if out.len() >= max {
            break;
        }
    }

    for film in films.iter().filter(|f| f.watchlist && !f.watched).take(12) {
        let key = seen_key(&film.title, film.year);
        if seen.contains(&key) || !used.insert(key) {
            continue;
        }
        out.push(json!({
            "t": film.title,
            "y": film.year,
            "src": "watchlist",
        }));
        if out.len() >= max {
            break;
        }
    }
    Ok(out)
}

fn load_films(db: &Database) -> Result<Vec<FilmRow>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT
              COALESCE(ml.movie_id, smr.normalized_title || ':' || IFNULL(CAST(smr.release_year AS TEXT), '')),
              COALESCE(m.canonical_title, json_extract(smr.raw_identity, '$.title'), smr.normalized_title),
              COALESCE(m.release_year, smr.release_year),
              ums.current_rating,
              COALESCE(ums.liked, 0),
              COALESCE(ums.watched, 0),
              COALESCE(ums.watchlist, smr.on_watchlist, 0),
              (SELECT COUNT(*) FROM viewings v WHERE v.source_movie_record_id = smr.id),
              m.genres_json,
              m.cast_json,
              m.crew_json,
              m.overview,
              m.runtime,
              m.tagline,
              m.similar_json,
              m.tmdb_id,
              COALESCE(m.poster_path, smr.cached_poster_url)
            FROM source_movie_records smr
            LEFT JOIN movie_links ml ON ml.source_movie_record_id = smr.id
            LEFT JOIN movies m ON m.id = ml.movie_id
            LEFT JOIN user_movie_state ums ON ums.source_movie_record_id = smr.id
            WHERE ums.current_rating IS NOT NULL
               OR ums.watched = 1
               OR ums.watchlist = 1
               OR smr.on_watchlist = 1
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let crew: Vec<String> = json_vec(row.get::<_, Option<String>>(10)?);
            Ok(FilmRow {
                key: row.get(0)?,
                title: display_title(&row.get::<_, String>(1)?),
                year: row.get(2)?,
                rating: row.get(3)?,
                liked: row.get::<_, i32>(4)? == 1,
                watched: row.get::<_, i32>(5)? == 1,
                watchlist: row.get::<_, i32>(6)? == 1,
                viewings: row.get::<_, i64>(7)? as u32,
                genres: json_vec(row.get::<_, Option<String>>(8)?),
                cast: json_vec(row.get::<_, Option<String>>(9)?),
                directors: crew_with(&crew, &["Director"]),
                writers: crew_with(&crew, &["Writer", "Screenplay", "Original Screenplay", "Story"]),
                cinematographers: crew_with(
                    &crew,
                    &["Director of Photography", "Cinematography"],
                ),
                composers: crew_with(&crew, &["Original Music Composer", "Music"]),
                overview: row.get(11)?,
                runtime: row.get(12)?,
                tagline: row.get(13)?,
                similar: parse_similar(row.get::<_, Option<String>>(14)?),
                poster: poster_url(row.get(16)?),
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn display_title(raw: &str) -> String {
    let (title, _) = parse_activity_payload(raw);
    if title.trim().is_empty() {
        raw.to_string()
    } else {
        title
    }
}

fn json_vec(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn crew_with(crew: &[String], jobs: &[&str]) -> Vec<String> {
    crew.iter()
        .filter_map(|entry| {
            let (name, job) = entry.rsplit_once(" (")?;
            let job = job.trim_end_matches(')');
            if jobs.iter().any(|j| *j == job) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn actor_names(cast: &[String]) -> Vec<String> {
    cast.iter()
        .map(|entry| {
            entry
                .split(" as ")
                .next()
                .unwrap_or(entry)
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_similar(raw: Option<String>) -> Vec<LibraryItem> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    if let Ok(items) = serde_json::from_str::<Vec<LibraryItem>>(&raw) {
        if items.iter().any(|item| !item.title.is_empty()) {
            return items;
        }
    }
    serde_json::from_str::<Vec<Value>>(&raw)
        .ok()
        .map(|vals| {
            vals.iter()
                .filter_map(tmdb::library_item_from_tmdb_value)
                .collect()
        })
        .unwrap_or_default()
}

fn person_stats<F>(films: &[FilmRow], pick: F, limit: usize) -> Vec<TasteStat>
where
    F: Fn(&FilmRow) -> Vec<String>,
{
    let mut map: HashMap<String, (u32, f64, u32)> = HashMap::new();
    for film in films {
        let Some(rating) = film.rating else { continue };
        for name in pick(film) {
            if name.is_empty() {
                continue;
            }
            let entry = map.entry(name).or_insert((0, 0.0, 0));
            entry.0 += 1;
            entry.1 += rating;
            entry.2 += 1;
        }
    }
    let mut stats: Vec<TasteStat> = map
        .into_iter()
        .filter(|(_, (c, _, _))| *c >= 2)
        .map(|(label, (count, sum, n))| TasteStat {
            label,
            count,
            avg: ((sum / n as f64) * 100.0).round() / 100.0,
        })
        .collect();
    stats.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.avg.partial_cmp(&a.avg).unwrap_or(std::cmp::Ordering::Equal))
    });
    stats.truncate(limit);
    stats
}

fn decade_stats(films: &[FilmRow]) -> Vec<TasteStat> {
    let mut map: HashMap<i32, (u32, f64)> = HashMap::new();
    for film in films {
        let Some(year) = film.year else { continue };
        let Some(rating) = film.rating else { continue };
        let decade = (year / 10) * 10;
        let entry = map.entry(decade).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += rating;
    }
    let mut stats: Vec<TasteStat> = map
        .into_iter()
        .map(|(decade, (count, sum))| TasteStat {
            label: format!("{decade}s"),
            count,
            avg: ((sum / count as f64) * 100.0).round() / 100.0,
        })
        .collect();
    stats.sort_by(|a, b| b.count.cmp(&a.count));
    stats.truncate(10);
    stats
}

fn runtime_stats(rated: &[&FilmRow]) -> Value {
    let mut buckets = [
        ("under 90", 0u32, 0.0),
        ("90-119", 0, 0.0),
        ("120-149", 0, 0.0),
        ("150+", 0, 0.0),
    ];
    for film in rated {
        let Some(rt) = film.runtime else { continue };
        let Some(rating) = film.rating else { continue };
        let idx = if rt < 90 {
            0
        } else if rt < 120 {
            1
        } else if rt < 150 {
            2
        } else {
            3
        };
        buckets[idx].1 += 1;
        buckets[idx].2 += rating;
    }
    json!(buckets
        .iter()
        .filter(|b| b.1 > 0)
        .map(|b| json!({"n": b.0, "c": b.1, "avg": ((b.2 / b.1 as f64) * 100.0).round() / 100.0}))
        .collect::<Vec<_>>())
}

fn seen_key(title: &str, year: Option<i32>) -> String {
    format!("{}|{}", title.trim().to_lowercase(), year.unwrap_or(0))
}

fn clip(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn seed(db: &Database) {
        db.conn()
            .execute(
                "INSERT INTO movies(id, canonical_title, release_year, genres_json, cast_json, crew_json, overview, runtime)
                 VALUES ('m1','Heat',1995,'[\"Crime\",\"Thriller\"]','[\"Al Pacino as Hanna\",\"Robert De Niro as McCauley\"]',
                 '[\"Michael Mann (Director)\",\"Dante Spinotti (Director of Photography)\"]','Cops and crooks',170)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO movies(id, canonical_title, release_year, genres_json, cast_json, crew_json, overview, runtime)
                 VALUES ('m2','The Room',2003,'[\"Drama\"]','[\"Tommy Wiseau as Johnny\"]',
                 '[\"Tommy Wiseau (Director)\"]','Oh hi Mark',99)",
                [],
            )
            .unwrap();
        for (id, key, title, year, movie) in [
            ("s1", "film:heat", "Heat", 1995, "m1"),
            ("s2", "film:room", "The Room", 2003, "m2"),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO source_movie_records(id, source_type, source_record_key, normalized_title, release_year, raw_identity, created_at)
                     VALUES (?1,'export',?2,?3,?4,?5,'2020-01-01')",
                    params![
                        id,
                        key,
                        title.to_lowercase(),
                        year,
                        format!("{{\"title\":\"{title}\"}}")
                    ],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO movie_links(source_movie_record_id, movie_id, match_state) VALUES (?1,?2,'confirmed')",
                    params![id, movie],
                )
                .unwrap();
        }
        db.conn()
            .execute(
                "INSERT INTO user_movie_state(source_movie_record_id, movie_id, watched, watchlist, liked, current_rating, projection_updated_at)
                 VALUES ('s1','m1',1,0,0,5,'2020-01-01')",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO user_movie_state(source_movie_record_id, movie_id, watched, watchlist, liked, current_rating, projection_updated_at)
                 VALUES ('s2','m2',1,0,0,0.5,'2020-01-01')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn corpus_splits_loved_and_hated_and_keeps_crew() {
        let db = Database::in_memory().unwrap();
        seed(&db);
        let corpus = build_corpus(&db, MODEL_DEEPSEEK).unwrap();
        assert_eq!(corpus.snapshot.rated_count, 2);
        assert_eq!(corpus.snapshot.loved_count, 1);
        assert_eq!(corpus.snapshot.hated_count, 1);
        let payload = corpus.payload.to_string();
        assert!(payload.contains("Heat"));
        assert!(payload.contains("The Room"));
        assert!(payload.contains("Michael Mann"));
        assert!(payload.contains("Dante Spinotti"));
        assert!(payload.contains("Crime"));
        assert!(corpus.payload.get("seen").is_none());
        assert!(estimate_tokens(&corpus.payload) < 28_000);
    }

    #[test]
    fn deepseek_budget_caps_a_huge_log() {
        let db = Database::in_memory().unwrap();
        for i in 0..180 {
            let id = format!("m{i}");
            let sid = format!("s{i}");
            let title = format!("Film Number {i} The Very Long Title Of A Movie");
            let overview = "A ".repeat(80);
            db.conn()
                .execute(
                    "INSERT INTO movies(id, canonical_title, release_year, genres_json, overview)
                     VALUES (?1,?2,1999,'[\"Drama\"]',?3)",
                    params![id, title, overview],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO source_movie_records(id, source_type, source_record_key, normalized_title, release_year, raw_identity, created_at)
                     VALUES (?1,'export',?2,?3,1999,'{}','2020-01-01')",
                    params![sid, format!("film:{i}"), title.to_lowercase()],
                )
                .unwrap();
            db.conn()
                .execute(
                    "INSERT INTO movie_links(source_movie_record_id, movie_id, match_state) VALUES (?1,?2,'confirmed')",
                    params![sid, format!("m{i}")],
                )
                .unwrap();
            let rating = if i < 90 { 5.0 } else { 1.0 };
            db.conn()
                .execute(
                    "INSERT INTO user_movie_state(source_movie_record_id, movie_id, watched, watchlist, liked, current_rating, projection_updated_at)
                     VALUES (?1,?2,1,0,0,?3,'2020-01-01')",
                    params![sid, format!("m{i}"), rating],
                )
                .unwrap();
        }
        let corpus = build_corpus(&db, MODEL_DEEPSEEK).unwrap();
        assert_eq!(corpus.snapshot.rated_count, 180);
        assert!(corpus.payload.get("seen").is_none());
        assert!(corpus.payload["reallyLiked"].as_array().unwrap().len() <= 180);
        assert!(estimate_tokens(&corpus.payload) < 80_000);
    }

    #[test]
    fn extract_json_unwraps_fences() {
        let raw = "```json\n{\"title\":\"Night owl\",\"summary\":\"x\",\"picks\":[]}\n```";
        let v = extract_json(raw).unwrap();
        assert_eq!(v["title"], "Night owl");
    }

    #[test]
    fn model_ids_map() {
        assert_eq!(default_model(), MODEL_QWEN);
        assert_eq!(normalize_model("Kimi K3"), MODEL_KIMI);
        assert_eq!(normalize_model("Gemini Flash"), MODEL_GEMINI);
        assert_eq!(normalize_model("deepseek"), MODEL_DEEPSEEK);
        assert_eq!(openrouter_model_id(MODEL_KIMI), "moonshotai/kimi-k3");
        assert_eq!(openrouter_model_id(MODEL_DEEPSEEK), "deepseek/deepseek-v4-flash");
        assert_eq!(openrouter_model_id(MODEL_QWEN), "qwen/qwen3.7-flash");
        assert_eq!(openrouter_model_id(MODEL_GEMINI), "google/gemini-3.7-flash");
        assert_eq!(model_catalog().len(), 4);
    }

    #[test]
    fn timeout_errors_are_rewritten_and_fall_back() {
        let raw = "A connection attempt failed because the connected party did not properly respond after a period of time, or established connection failed because connected host has failed to respond. (os error 10060)";
        assert!(is_timeout_error(raw));
        assert!(should_fallback(MODEL_KIMI, raw));
        assert!(!should_fallback(MODEL_QWEN, raw));
        let msg = friendly_err(raw, MODEL_KIMI);
        assert!(msg.contains("Kimi K3"));
        assert!(!msg.contains("10060"));
    }
}
