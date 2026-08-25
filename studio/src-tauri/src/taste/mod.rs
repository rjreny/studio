pub mod dimensions;
pub mod discover;
pub mod eval;
pub mod features;
pub mod preference;
pub mod provenance;
pub mod reason;
pub mod retrieve;
pub mod score;
pub mod shortlist;
pub mod validate;
pub mod library_fixture;

#[cfg(test)]
mod real_run;

use crate::catalog::tmdb;
use crate::models::JobProgress;
use crate::storage::db::Database;
use crate::taste::features::{
    build_profile, observations_from_film, FeatureFamily, FeatureProfile, PORTABLE_CONTEXTUAL,
};
use crate::taste::dimensions::ModeFilm;
use crate::taste::preference::MIN_RATINGS;
use crate::taste::reason::{
    call1_payload, call2_payload, empty_critic, parse_critic, parse_reasoner, CALL1_SYSTEM,
    CALL2_SYSTEM, CriticReport, ReasonerPick,
};
use crate::taste::retrieve::{attach_signals, load_films, retrieve, seen_keys, FilmRecord};
use crate::taste::score::{score_all, ScoredCandidate};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const KEYRING_SERVICE: &str = "studio";
const KEYRING_USER: &str = "openrouter_api_key";
const META_REPORT: &str = "taste_report";
const META_MODEL: &str = "taste_model";
const META_WEB: &str = "taste_web";
const OPENROUTER_CHAT: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_KEY: &str = "https://openrouter.ai/api/v1/key";
const MODEL_LLAMA: &str = "llama";
const MODEL_DEEPSEEK: &str = "deepseek";
const MODEL_QWEN: &str = "qwen";
const MODEL_NEMO: &str = "nemo";

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
    #[serde(default)]
    pub rhymes_with: Vec<String>,
    pub film_id: Option<String>,
    pub tmdb_id: Option<i64>,
    pub source: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteStat {
    pub label: String,
    pub count: u32,
    pub avg: f64,
    #[serde(default)]
    pub affinity: Option<f64>,
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
    #[serde(default)]
    pub cinematographers: Vec<TasteStat>,
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

pub fn default_model() -> String {
    MODEL_LLAMA.to_string()
}

pub fn normalize_model(raw: &str) -> String {
    let compact = raw.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    if compact.contains("deepseek") {
        MODEL_DEEPSEEK.into()
    } else if compact.contains("qwen") {
        MODEL_QWEN.into()
    } else if compact.contains("nemo") || compact.contains("mistral") {
        MODEL_NEMO.into()
    } else {
        MODEL_LLAMA.into()
    }
}

fn openrouter_model_id(model: &str) -> &'static str {
    match model {
        MODEL_DEEPSEEK => "deepseek/deepseek-chat",
        MODEL_QWEN => "qwen/qwen-2.5-72b-instruct",
        MODEL_NEMO => "mistralai/mistral-nemo",
        _ => "meta-llama/llama-3.3-70b-instruct",
    }
}

fn model_label(model: &str) -> &'static str {
    match model {
        MODEL_DEEPSEEK => "DeepSeek V3",
        MODEL_QWEN => "Qwen2.5 72B",
        MODEL_NEMO => "Mistral Nemo",
        _ => "Llama 3.3 70B",
    }
}

pub fn model_catalog() -> Vec<TasteModelInfo> {
    vec![
        TasteModelInfo {
            id: MODEL_LLAMA.into(),
            label: "Llama 3.3 70B".into(),
            blurb: "Reasons over a scored shortlist. 128k context, cheap default.".into(),
            context: "128k".into(),
            cost: "cheap".into(),
        },
        TasteModelInfo {
            id: MODEL_DEEPSEEK.into(),
            label: "DeepSeek V3".into(),
            blurb: "Strong critic pass. Some hosts cap context, so the shortlist stays tight.".into(),
            context: "32–164k".into(),
            cost: "cheap".into(),
        },
        TasteModelInfo {
            id: MODEL_NEMO.into(),
            label: "Mistral Nemo".into(),
            blurb: "Cheapest 128k option. Weaker editorial take than the 70B models.".into(),
            context: "128k".into(),
            cost: "cheapest".into(),
        },
        TasteModelInfo {
            id: MODEL_QWEN.into(),
            label: "Qwen2.5 72B".into(),
            blurb: "The Qwen your key can actually reach. Costs more than Llama 3.3.".into(),
            context: "32k".into(),
            cost: "mid".into(),
        },
    ]
}

fn request_timeout(_model: &str) -> Duration {
    Duration::from_secs(150)
}

fn fallback_model(model: &str) -> &'static str {
    if model == MODEL_LLAMA {
        MODEL_DEEPSEEK
    } else {
        MODEL_LLAMA
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
        .set("User-Agent", "Studio/0.7 (local film app)")
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
    let Some(key) = get_api_key()? else {
        return with_prefs(db, empty_status());
    };
    let mut status = empty_status();
    status.stored = true;
    status.valid = Some(true);
    let _ = key;
    with_prefs(db, status)
}

pub fn key_status(db: &Database) -> Result<TasteKeyStatus, String> {
    match get_api_key()? {
        None => with_prefs(db, empty_status()),
        Some(key) => {
            let mut status = probe_key(&key)?;
            status.stored = true;
            with_prefs(db, status)
        }
    }
}

pub fn store_api_key(db: &Database, key: &str) -> Result<TasteKeyStatus, String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("Paste an OpenRouter key first.".into());
    }
    let mut status = probe_key(trimmed)?;
    if status.valid == Some(false) {
        return Ok(status);
    }
    set_api_key(trimmed)?;
    status.stored = true;
    with_prefs(db, status)
}

fn snapshot_of(films: &[FilmRecord], profile: Option<&FeatureProfile>) -> TasteSnapshot {
    let rated: Vec<&FilmRecord> = films.iter().filter(|f| f.rating.is_some()).collect();
    let loved = rated
        .iter()
        .filter(|f| f.rating.unwrap() >= 4.5)
        .count() as u32;
    let hated = rated
        .iter()
        .filter(|f| f.rating.unwrap() <= 2.5)
        .count() as u32;
    let avg = if rated.is_empty() {
        None
    } else {
        Some(
            (rated.iter().map(|f| f.rating.unwrap() as f64).sum::<f64>() / rated.len() as f64
                * 100.0)
                .round()
                / 100.0,
        )
    };
    let stats = |family: FeatureFamily| -> Vec<TasteStat> {
        profile
            .map(|p| {
                p.affinities
                    .iter()
                    .filter(|a| {
                        a.key.family == family
                            && (!family.is_contextual() || a.portability >= PORTABLE_CONTEXTUAL)
                    })
                    .take(16)
                    .map(|a| TasteStat {
                        label: a.key.name.clone(),
                        count: a.appearances,
                        avg: (a.preference_mean as f64 * 100.0).round() / 100.0,
                        affinity: Some((a.scoring_affinity() as f64 * 100.0).round() / 100.0),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    TasteSnapshot {
        rated_count: rated.len() as u32,
        loved_count: loved,
        hated_count: hated,
        avg_rating: avg,
        genres: stats(FeatureFamily::Genre),
        decades: stats(FeatureFamily::Decade),
        directors: stats(FeatureFamily::Director),
        actors: stats(FeatureFamily::Actor),
        cinematographers: stats(FeatureFamily::Cinematographer),
    }
}

pub(crate) fn feature_profile_from_films(films: &[FilmRecord]) -> FeatureProfile {
    let mut obs = Vec::new();
    for film in films {
        let (Some(rating), Some(signal)) = (film.rating, film.signal.as_ref()) else {
            continue;
        };
        obs.extend(observations_from_film(
            &film.title,
            rating,
            film.tmdb_id,
            signal,
            film.age_years,
            &film.genres,
            &film.credits,
            &film.keywords,
            film.year,
            film.runtime,
        ));
    }
    let mut profile = build_profile(&obs);
    let inputs: Vec<ModeFilm<'_>> = films
        .iter()
        .map(|f| ModeFilm {
            title: &f.title,
            rating: f.rating,
            tmdb_id: f.tmdb_id,
            genres: &f.genres,
            credits: &f.credits,
            keywords: &f.keywords,
            signal: f.signal.as_ref(),
            age_years: f.age_years,
        })
        .collect();
    let (dimensions, modes, mode_shifts) = crate::taste::dimensions::derive(&inputs);
    profile.dimensions = dimensions;
    profile.modes = modes;
    profile.mode_shifts = mode_shifts;
    profile
}

pub fn load_state(db: &Database) -> Result<TasteState, String> {
    let mut films = load_films(db)?;
    attach_signals(&mut films);
    let profile = feature_profile_from_films(&films);
    let report = db
        .get_meta(META_REPORT)?
        .and_then(|raw| serde_json::from_str(&raw).ok());
    Ok(TasteState {
        key: stored_status(db)?,
        snapshot: snapshot_of(&films, Some(&profile)),
        report,
    })
}

pub fn analyze(
    db: &Database,
    progress: &mut dyn FnMut(JobProgress),
) -> Result<TasteReport, String> {
    let key = get_api_key()?.ok_or_else(|| {
        "Add an OpenRouter key in Settings. Llama 3.3 70B is the cheap default.".to_string()
    })?;
    let model = stored_model(db)?;
    let web = stored_web(db)?;
    progress(JobProgress {
        job: "taste".into(),
        label: "Reading your log…".into(),
        current: 1,
        total: 6,
        ..Default::default()
    });
    let mut films = load_films(db)?;
    let rated = films.iter().filter(|f| f.rating.is_some()).count();
    if rated < MIN_RATINGS {
        return Err("Rate at least 8 films first so the agent has edges to work with.".into());
    }
    retrieve::enrich_rated_library(db, &mut films, 40);
    attach_signals(&mut films);
    let profile = feature_profile_from_films(&films);
    let seen = seen_keys(&films);
    progress(JobProgress {
        job: "taste".into(),
        label: "Scoring candidates…".into(),
        current: 2,
        total: 6,
        ..Default::default()
    });
    let mut candidates = retrieve(db, &films, &profile, &seen)?;
    retrieve::enrich_missing(db, &mut candidates, 40);
    let ranked = score_all(&profile, &candidates);
    let short = shortlist::shortlist(&ranked);
    if short.is_empty() {
        return Err("Could not build a candidate shortlist. Finish matching posters, then try again.".into());
    }

    progress(JobProgress {
        job: "taste".into(),
        label: format!("Asking {} to critique the shortlist…", model_label(&model)),
        current: 3,
        total: 6,
        ..Default::default()
    });
    let mut used_model = model.clone();
    let mut used_web = false;
    let mut note = None;
    let critic = match run_json(&key, &model, CALL1_SYSTEM, &call1_payload(&films, &profile, &short), false)
    {
        Ok((raw, _)) => match parse_critic(&extract_json(&raw)?) {
            Ok(c) => c,
            Err(_) => empty_critic(),
        },
        Err(err) if should_fallback(&err) => {
            let next = fallback_model(&model);
            used_model = next.to_string();
            note = Some(format!(
                "Fell back to {} after {} stalled on the critic pass.",
                model_label(next),
                model_label(&model)
            ));
            run_json(&key, next, CALL1_SYSTEM, &call1_payload(&films, &profile, &short), false)
                .ok()
                .and_then(|(raw, _)| extract_json(&raw).ok())
                .and_then(|v| parse_critic(&v).ok())
                .unwrap_or_else(empty_critic)
        }
        Err(_) => empty_critic(),
    };

    progress(JobProgress {
        job: "taste".into(),
        label: if web {
            "Running targeted discovery…".into()
        } else {
            "Skipping web discovery…".into()
        },
        current: 4,
        total: 6,
        ..Default::default()
    });
    let mut discoveries = Vec::new();
    if web {
        for q in critic.discovery_queries.iter().take(3) {
            match run_json(
                &key,
                &used_model,
                "Return JSON {\"titles\":[{\"title\":\"...\",\"year\":1999}]} for this research query. At most 5 real films. JSON only.",
                &json!({ "query": q.query, "targetFacet": q.target_facet, "why": q.why }),
                true,
            ) {
                Ok((raw, actually_web)) => {
                    used_web = used_web || actually_web;
                    if let Ok(v) = extract_json(&raw) {
                        let titles = discover::parse_search_titles(&v);
                        discoveries.extend(discover::materialize(
                            db,
                            &titles,
                            &q.query,
                            &seen,
                            &profile,
                        ));
                    }
                }
                Err(_) => {}
            }
        }
        discoveries.truncate(3);
    }

    progress(JobProgress {
        job: "taste".into(),
        label: format!("Asking {} for the final 12…", model_label(&used_model)),
        current: 5,
        total: 6,
        ..Default::default()
    });
    let mut call2_body = call2_payload(&films, &profile, &short, &critic, &discoveries);
    let mut reasoner = run_reasoner(&key, &used_model, &call2_body)?;
    let mut validated = validate::hard_validate(
        &reasoner.picks,
        &short,
        &discoveries,
        &seen,
        profile.modes.len(),
    );
    if !validated.warnings.is_empty() && !validated.narrow_profile {
        call2_body["diversityWarnings"] = json!(validated
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>());
        if let Ok(repaired) = run_reasoner(&key, &used_model, &call2_body) {
            reasoner = repaired;
            validated = validate::hard_validate(
                &reasoner.picks,
                &short,
                &discoveries,
                &seen,
                profile.modes.len(),
            );
        }
    }

    progress(JobProgress {
        job: "taste".into(),
        label: "Matching posters…".into(),
        current: 6,
        total: 6,
        ..Default::default()
    });
    let picks = to_taste_picks(db, &reasoner.picks, &validated.picks, &critic, &discoveries)?;
    let report = TasteReport {
        title: if reasoner.title.trim().is_empty() {
            "Taste".into()
        } else {
            reasoner.title
        },
        summary: reasoner.summary,
        affinities: reasoner.affinities,
        aversions: reasoner.aversions,
        dimensions: reasoner.dimensions,
        picks,
        model: model_label(&used_model).into(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        rated_count: rated as u32,
        web_used: used_web,
        note,
    };
    db.set_meta(META_REPORT, &serde_json::to_string(&report).unwrap_or_default())?;
    Ok(report)
}

fn run_reasoner(key: &str, model: &str, payload: &Value) -> Result<reason::ReasonerReport, String> {
    let (raw, _) = match run_json(key, model, CALL2_SYSTEM, payload, false) {
        Ok(v) => v,
        Err(err) if should_fallback(&err) => {
            let next = fallback_model(model);
            run_json(key, next, CALL2_SYSTEM, payload, false).map_err(|e| friendly_err(&e, next))?
        }
        Err(err) => return Err(friendly_err(&err, model)),
    };
    let parsed = extract_json(&raw).and_then(|v| parse_reasoner(&v));
    match parsed {
        Ok(r) if !r.picks.is_empty() || !r.title.is_empty() => Ok(r),
        _ => {
            let next = fallback_model(model);
            let (raw2, _) = run_json(key, next, CALL2_SYSTEM, payload, false)
                .map_err(|e| friendly_err(&e, next))?;
            parse_reasoner(&extract_json(&raw2)?).map_err(|e| {
                format!(
                    "{} wrote invalid JSON ({e}). Try Llama 3.3 70B with web search off.",
                    model_label(next)
                )
            })
        }
    }
}

fn to_taste_picks(
    db: &Database,
    reasoner_picks: &[ReasonerPick],
    validated: &[ScoredCandidate],
    critic: &CriticReport,
    discoveries: &[ScoredCandidate],
) -> Result<Vec<TastePick>, String> {
    let mut out = Vec::new();
    for scored in validated {
        let rp = reasoner_picks.iter().find(|p| {
            (!p.id.is_empty()
                && scored.candidate.tmdb_id.map(|id| format!("tmdb:{id}")).as_deref() == Some(p.id.as_str()))
                || p.title.eq_ignore_ascii_case(&scored.candidate.title)
        });
        let mut poster = scored.candidate.poster.clone();
        let mut film_id = scored.candidate.tmdb_id.map(|id| format!("tmdb:{id}"));
        if poster.is_none() {
            if let Some(id) = scored.candidate.tmdb_id {
                if let Ok(Some(hit)) = tmdb::lookup_movie(&scored.candidate.title, scored.candidate.year)
                {
                    poster = hit.poster;
                    let _ = id;
                }
            }
        }
        if let Some(tid) = scored.candidate.tmdb_id {
            if let Ok(Some(local)) = library_id_for_tmdb(db, tid) {
                film_id = Some(local);
            }
        }
        let origin = if discoveries.iter().any(|d| {
            d.candidate.tmdb_id.is_some() && d.candidate.tmdb_id == scored.candidate.tmdb_id
        }) {
            "discovery"
        } else {
            "shortlist"
        };
        let assess = critic.candidate_assessments.iter().find(|a| {
            scored
                .candidate
                .tmdb_id
                .map(|id| a.id == format!("tmdb:{id}"))
                .unwrap_or(false)
        });
        let _provenance = crate::taste::provenance::RecommendationProvenance {
            tmdb_id: scored.candidate.tmdb_id,
            origin: crate::taste::provenance::origin_from_sources(&scored.candidate.sources),
            retrieval_sources: scored.candidate.sources.clone(),
            deterministic_score: scored.score.clone(),
            llm_mode: crate::taste::provenance::RecommendationMode::parse(
                rp.map(|p| p.mode.as_str()).unwrap_or(origin),
            ),
            seed_films: scored.evidence.clone(),
            positive_features: scored.positive_features.clone(),
            negative_features_considered: scored.negative_features.clone(),
            call1_fit: assess.map(|a| a.fit.clone()),
            call1_concerns: assess.map(|a| a.concerns.clone()).unwrap_or_default(),
        };
        let _ = &_provenance;
        out.push(TastePick {
            title: scored.candidate.title.clone(),
            year: scored.candidate.year,
            poster,
            why: rp.map(|p| p.why.clone()).unwrap_or_default(),
            rhymes_with: rp.map(|p| p.rhymes_with.clone()).unwrap_or_else(|| scored.evidence.clone()),
            film_id,
            tmdb_id: scored.candidate.tmdb_id,
            source: origin.into(),
            reasons: scored.reasons.clone(),
            evidence: scored.evidence.clone(),
            mode: rp.map(|p| p.mode.clone()).filter(|s| !s.is_empty()),
        });
    }
    Ok(out)
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

fn run_json(
    key: &str,
    model: &str,
    system: &str,
    payload: &Value,
    web: bool,
) -> Result<(String, bool), String> {
    match send_chat(key, model, system, payload, web) {
        Ok(raw) => Ok((raw, web)),
        Err(err) if web && (tools_unsupported(&err) || is_guardrail(&err)) => {
            send_chat(key, model, system, payload, false).map(|raw| (raw, false))
        }
        Err(err) => Err(err),
    }
}

fn send_chat(
    key: &str,
    model: &str,
    system: &str,
    payload: &Value,
    web: bool,
) -> Result<String, String> {
    let timeout = request_timeout(model);
    let mut body = json!({
        "model": openrouter_model_id(model),
        "temperature": 0.35,
        "max_tokens": 6000,
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
        .set("User-Agent", "Studio/0.7 (local film app)")
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
    let v: Value = serde_json::from_str(&response)
        .map_err(|e| format!("OpenRouter returned non-JSON: {e}"))?;
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
    lower.contains("10060") || lower.contains("timed out") || lower.contains("timeout") || lower.contains("time out")
}

fn is_guardrail(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("no endpoints") || lower.contains("guardrail") || lower.contains("data policy")
}

fn tools_unsupported(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("does not support")
        || lower.contains("tool use")
        || lower.contains("tools are not")
        || lower.contains("unknown tool")
}

fn should_fallback(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    is_timeout_error(err)
        || is_guardrail(err)
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("524")
        || lower.contains("connection reset")
        || lower.contains("failed to connect")
}

fn friendly_err(err: &str, model: &str) -> String {
    if is_guardrail(err) {
        return format!(
            "{} has no OpenRouter endpoint that matches your privacy settings. Taste retries without web search, then with another model from your list. To allow more models, open openrouter.ai/settings/privacy",
            model_label(model)
        );
    }
    if is_timeout_error(err) {
        return format!(
            "{} did not finish in time. Taste will retry with another model from your list.",
            model_label(model)
        );
    }
    err.to_string()
}

pub fn extract_json(raw: &str) -> Result<Value, String> {
    let cleaned = strip_think(&strip_fences(raw));
    let object = first_object(&cleaned).ok_or_else(|| "Model did not return JSON".to_string())?;
    for candidate in [
        object.clone(),
        strip_trailing_commas(&object),
        close_truncated(&strip_trailing_commas(&object)),
    ] {
        if let Ok(v) = serde_json::from_str::<Value>(&candidate) {
            if v.is_object() {
                return Ok(v);
            }
        }
    }
    Err("Model did not return JSON".into())
}

fn strip_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = if let Some(rest) = trimmed.strip_prefix("```json") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest
    } else if let Some(start) = trimmed.find("```json") {
        &trimmed[start + 7..]
    } else {
        trimmed
    };
    inner.trim().trim_end_matches('`').trim().into()
}

fn strip_think(raw: &str) -> String {
    let mut s = raw.replace(['\u{201c}', '\u{201d}'], "\"");
    if let Some(end) = s.find("</think>") {
        s = s[end + 8..].to_string();
    }
    s
}

fn first_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut out = String::new();
    for c in s[start..].chars() {
        out.push(c);
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(out);
                }
            }
            _ => {}
        }
    }
    Some(out)
}

fn strip_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escape = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn close_truncated(s: &str) -> String {
    let mut out = s.trim_end().to_string();
    if out.ends_with(',') {
        out.pop();
    }
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escape = false;
    for c in out.chars() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                stack.pop();
            }
            _ => {}
        }
    }
    if in_string {
        out.push('"');
    }
    while let Some(closer) = stack.pop() {
        out.push(closer);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_unwraps_fences() {
        let raw = "```json\n{\"title\":\"Night owl\",\"summary\":\"x\",\"picks\":[]}\n```";
        let v = extract_json(raw).unwrap();
        assert_eq!(v["title"], "Night owl");
    }

    #[test]
    fn model_ids_map() {
        assert_eq!(default_model(), MODEL_LLAMA);
        assert_eq!(normalize_model("deepseek"), MODEL_DEEPSEEK);
        assert_eq!(openrouter_model_id(MODEL_LLAMA), "meta-llama/llama-3.3-70b-instruct");
        assert_eq!(model_catalog().len(), 4);
    }
}
