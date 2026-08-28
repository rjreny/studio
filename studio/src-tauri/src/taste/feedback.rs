use crate::storage::db::Database;
use crate::taste::explain::MatchedFeatureView;
use crate::taste::features::{keyword_strength, FeatureProfile, KeywordStrength};
use crate::taste::score::ScoredCandidate;
use chrono::{Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const ACTION_INTERESTED: &str = "interested";
pub const ACTION_REJECTED: &str = "rejected";
pub const ACTION_SEEN: &str = "seen";
pub const MOOD_SCOPE_MOVIE_ONLY: &str = "this_movie_only";
pub const MOOD_SCOPE_KIND_RIGHT_NOW: &str = "this_kind_right_now";
pub const FEEDBACK_SIGNAL_WEIGHT_V1: f32 = 0.20;
pub const FEEDBACK_ADJUSTMENT_CAP: f32 = 0.25;
pub const FEEDBACK_SIGNAL_VERSION: &str = "feedback-signal-v1";

const ALLOWED_REASONS: &[&str] = &[
    "already_seen_disliked",
    "not_this_kind",
    "wrong_connection",
    "not_in_the_mood",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteMoodSignature {
    #[serde(default)]
    pub modes: Vec<String>,
    #[serde(default)]
    pub thematic_keywords: Vec<String>,
}

impl TasteMoodSignature {
    pub fn element_count(&self) -> usize {
        self.elements().len()
    }

    fn elements(&self) -> HashSet<String> {
        self.modes
            .iter()
            .map(|mode| format!("mode:{}", normalize_signature_part(mode)))
            .chain(self.thematic_keywords.iter().map(|keyword| {
                format!("keyword:{}", normalize_signature_part(keyword))
            }))
            .filter(|part| !part.ends_with(':'))
            .collect()
    }

    fn overlap_count(&self, other: &Self) -> usize {
        self.elements().intersection(&other.elements()).count()
    }
}

fn normalize_signature_part(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureExposureCount {
    pub feature_key: String,
    pub exposures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteAttribution {
    pub exposure_id: String,
    pub run_id: String,
    pub tmdb_id: i64,
    pub title: String,
    pub evidence_grade: String,
    #[serde(default)]
    pub cited_positive: Vec<MatchedFeatureView>,
    #[serde(default)]
    pub cited_negative: Vec<MatchedFeatureView>,
    #[serde(default)]
    pub seed_films: Vec<String>,
    #[serde(default)]
    pub semantic_fit: f32,
    #[serde(default)]
    pub diversity_adjustment: f32,
    #[serde(default)]
    pub retrieval_source: String,
    #[serde(default)]
    pub ranking_rationale: Vec<String>,
    #[serde(default)]
    pub mood_signature: TasteMoodSignature,
    #[serde(default)]
    pub prior_candidate_exposures: u32,
    #[serde(default)]
    pub prior_feature_exposures: Vec<FeatureExposureCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackAdjustment {
    pub feature_key: String,
    pub requested_delta: f32,
    pub applied_delta: f32,
    pub pre_adjustment: f32,
    pub resulting_adjustment: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteFeedbackRequest {
    pub tmdb_id: i64,
    pub action: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub exposure_id: Option<String>,
    #[serde(default)]
    pub target_feature_key: Option<String>,
    #[serde(default)]
    pub mood_scope: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureExposureMetric {
    pub feature_key: String,
    pub exposures: u32,
    pub feedback_events: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteObservationSummary {
    pub feedback_events: u32,
    pub later_outcomes: u32,
    pub feedback_reasons: u32,
    pub exposure_count: u32,
    pub mood_signature_eligible: u32,
    pub mood_fallbacks: u32,
    pub phase_two_unlocked: bool,
    #[serde(default)]
    pub feature_exposure: Vec<FeatureExposureMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteFeedback {
    pub content_key: String,
    pub tmdb_id: i64,
    pub media_kind: String,
    pub action: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub suppressed_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn content_key(tmdb_id: i64) -> String {
    format!("movie:tmdb:{tmdb_id}")
}

fn validate_action(action: &str) -> Result<(), String> {
    match action {
        ACTION_INTERESTED | ACTION_REJECTED | ACTION_SEEN => Ok(()),
        _ => Err(format!("unknown feedback action: {action}")),
    }
}

fn validate_reason(reason: &Option<String>) -> Result<(), String> {
    match reason {
        None => Ok(()),
        Some(r) if r.is_empty() => Ok(()),
        Some(r) if ALLOWED_REASONS.contains(&r.as_str()) => Ok(()),
        Some(r) => Err(format!("unknown feedback reason: {r}")),
    }
}

pub fn set_feedback(
    db: &Database,
    tmdb_id: i64,
    action: &str,
    reason: Option<String>,
) -> Result<TasteFeedback, String> {
    validate_action(action)?;
    let reason = reason.filter(|r| !r.is_empty());
    validate_reason(&reason)?;
    let key = content_key(tmdb_id);
    let now = Utc::now().to_rfc3339();
    let existing = get_one(db, &key)?;
    let created = existing
        .as_ref()
        .map(|r| r.created_at.clone())
        .unwrap_or_else(|| now.clone());
    db.conn()
        .execute(
            r#"
            INSERT INTO taste_feedback (content_key, tmdb_id, media_kind, action, reason, suppressed_until, created_at, updated_at)
            VALUES (?1, ?2, 'movie', ?3, ?4, NULL, ?5, ?6)
            ON CONFLICT(content_key) DO UPDATE SET
              action = excluded.action,
              reason = excluded.reason,
              suppressed_until = NULL,
              updated_at = excluded.updated_at
            "#,
            params![key, tmdb_id, action, reason, created, now],
        )
        .map_err(|e| e.to_string())?;
    get_one(db, &key)?.ok_or_else(|| "feedback row missing after upsert".into())
}

pub fn clear_feedback(db: &Database, tmdb_id: i64) -> Result<(), String> {
    db.conn()
        .execute(
            "DELETE FROM taste_feedback WHERE content_key = ?1",
            params![content_key(tmdb_id)],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_feedback(db: &Database) -> Result<Vec<TasteFeedback>, String> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT content_key, tmdb_id, media_kind, action, reason, suppressed_until, created_at, updated_at
             FROM taste_feedback",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TasteFeedback {
                content_key: row.get(0)?,
                tmdb_id: row.get(1)?,
                media_kind: row.get(2)?,
                action: row.get(3)?,
                reason: row.get(4)?,
                suppressed_until: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn get_one(db: &Database, key: &str) -> Result<Option<TasteFeedback>, String> {
    db.conn()
        .query_row(
            "SELECT content_key, tmdb_id, media_kind, action, reason, suppressed_until, created_at, updated_at
             FROM taste_feedback WHERE content_key = ?1",
            params![key],
            |row| {
                Ok(TasteFeedback {
                    content_key: row.get(0)?,
                    tmdb_id: row.get(1)?,
                    media_kind: row.get(2)?,
                    action: row.get(3)?,
                    reason: row.get(4)?,
                    suppressed_until: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional_row()
}

pub fn hide_ids(rows: &[TasteFeedback]) -> std::collections::HashSet<i64> {
    rows.iter()
        .filter(|r| r.action == ACTION_REJECTED || r.action == ACTION_SEEN)
        .map(|r| r.tmdb_id)
        .collect()
}

pub fn record_exposure(db: &Database, mut attribution: TasteAttribution) -> Result<TasteAttribution, String> {
    let prior_candidate_exposures: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM taste_recommendation_exposures WHERE tmdb_id = ?1",
            params![attribution.tmdb_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    attribution.prior_candidate_exposures = prior_candidate_exposures.max(0) as u32;
    let feature_keys = attribution_feature_keys(&attribution);
    attribution.prior_feature_exposures = feature_keys
        .iter()
        .map(|feature_key| {
            let exposures: i64 = db
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM taste_exposure_features WHERE feature_key = ?1",
                    params![feature_key],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            FeatureExposureCount {
                feature_key: feature_key.clone(),
                exposures: exposures.max(0) as u32,
            }
        })
        .collect();
    let now = Utc::now().to_rfc3339();
    let snapshot_json = serde_json::to_string(&attribution).map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "INSERT INTO taste_recommendation_exposures (id, run_id, tmdb_id, title, snapshot_json, prior_candidate_exposures, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![attribution.exposure_id, attribution.run_id, attribution.tmdb_id, attribution.title, snapshot_json, attribution.prior_candidate_exposures, now],
        )
        .map_err(|e| e.to_string())?;
    for feature_key in feature_keys {
        db.conn()
            .execute(
                "INSERT OR IGNORE INTO taste_exposure_features (exposure_id, feature_key) VALUES (?1, ?2)",
                params![attribution.exposure_id, feature_key],
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(attribution)
}

fn attribution_feature_keys(attribution: &TasteAttribution) -> Vec<String> {
    attribution
        .cited_positive
        .iter()
        .chain(attribution.cited_negative.iter())
        .filter_map(|feature| (!feature.feature_key.is_empty()).then(|| feature.feature_key.clone()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

pub fn set_feedback_with_exposure(
    db: &Database,
    request: TasteFeedbackRequest,
) -> Result<TasteFeedback, String> {
    validate_action(&request.action)?;
    let reason = request.reason.clone().filter(|value| !value.is_empty());
    validate_reason(&reason)?;
    let exposure_id = request
        .exposure_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "feedback must reference a displayed recommendation".to_string())?;
    let attribution = load_exposure(db, exposure_id)?;
    if attribution.tmdb_id != request.tmdb_id {
        return Err("feedback does not match its displayed recommendation".into());
    }
    let target_feature_key = request
        .target_feature_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let target = validate_target(&attribution, reason.as_deref(), target_feature_key.as_deref())?;
    let (mood_scope, mood_fallback, expires_at) = validate_mood_scope(
        &attribution,
        reason.as_deref(),
        request.mood_scope.as_deref(),
    )?;
    let current = active_feedback_adjustments(db)?;
    let adjustments = requested_adjustments(
        &attribution,
        &request.action,
        reason.as_deref(),
        target,
        &current,
    );
    let feature_snapshot: Vec<MatchedFeatureView> = adjustments
        .iter()
        .filter_map(|adjustment| feature_in_attribution(&attribution, &adjustment.feature_key).cloned())
        .collect();
    let now = Utc::now().to_rfc3339();
    upsert_feedback_state(
        db,
        request.tmdb_id,
        &request.action,
        reason.as_deref(),
        expires_at.as_deref(),
        &now,
    )?;
    db.conn()
        .execute(
            "INSERT INTO taste_feedback_events (id, exposure_id, tmdb_id, action, reason, target_feature_key, mood_scope, mood_fallback, requested_adjustments_json, applied_adjustments_json, feature_snapshot_json, feedback_signal_version, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![Uuid::new_v4().to_string(), exposure_id, request.tmdb_id, request.action, reason, target_feature_key, mood_scope, if mood_fallback { 1 } else { 0 }, serde_json::to_string(&adjustments).unwrap_or_else(|_| "[]".into()), serde_json::to_string(&adjustments).unwrap_or_else(|_| "[]".into()), serde_json::to_string(&feature_snapshot).unwrap_or_else(|_| "[]".into()), FEEDBACK_SIGNAL_VERSION, expires_at, now],
        )
        .map_err(|e| e.to_string())?;
    crate::taste::cache::invalidate_snapshot(db)?;
    get_one(db, &content_key(request.tmdb_id))?.ok_or_else(|| "feedback row missing after save".into())
}

fn load_exposure(db: &Database, exposure_id: &str) -> Result<TasteAttribution, String> {
    let raw: String = db
        .conn()
        .query_row(
            "SELECT snapshot_json FROM taste_recommendation_exposures WHERE id = ?1",
            params![exposure_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "recommendation exposure no longer exists".to_string())?;
    serde_json::from_str(&raw).map_err(|_| "recommendation exposure is invalid".to_string())
}

fn validate_target<'a>(
    attribution: &'a TasteAttribution,
    reason: Option<&str>,
    target_feature_key: Option<&str>,
) -> Result<Option<&'a MatchedFeatureView>, String> {
    let target_required = matches!(reason, Some("wrong_connection") | Some("not_this_kind"));
    if target_required && target_feature_key.is_none() {
        return Err("choose a cited bridge for this feedback".into());
    }
    let target = target_feature_key.and_then(|key| feature_in_attribution(attribution, key));
    if target_feature_key.is_some() && target.is_none() {
        return Err("feedback may only target a citeable bridge shown on this card".into());
    }
    Ok(target)
}

fn feature_in_attribution<'a>(
    attribution: &'a TasteAttribution,
    feature_key: &str,
) -> Option<&'a MatchedFeatureView> {
    attribution
        .cited_positive
        .iter()
        .find(|feature| feature.citeable && feature.feature_key == feature_key)
}

fn validate_mood_scope(
    attribution: &TasteAttribution,
    reason: Option<&str>,
    raw_scope: Option<&str>,
) -> Result<(Option<String>, bool, Option<String>), String> {
    if reason != Some("not_in_the_mood") {
        if raw_scope.is_some() {
            return Err("mood scope is only valid for not-in-the-mood feedback".into());
        }
        return Ok((None, false, None));
    }
    let scope = raw_scope.unwrap_or(MOOD_SCOPE_MOVIE_ONLY);
    match scope {
        MOOD_SCOPE_MOVIE_ONLY => Ok((Some(scope.into()), false, None)),
        MOOD_SCOPE_KIND_RIGHT_NOW => {
            let fallback = attribution.mood_signature.element_count() < 2;
            let expires_at = (!fallback).then(|| (Utc::now() + Duration::days(30)).to_rfc3339());
            Ok((Some(scope.into()), fallback, expires_at))
        }
        _ => Err("unknown mood scope".into()),
    }
}

fn requested_adjustments(
    attribution: &TasteAttribution,
    action: &str,
    reason: Option<&str>,
    target: Option<&MatchedFeatureView>,
    current: &HashMap<String, f32>,
) -> Vec<FeedbackAdjustment> {
    let requests: Vec<(String, f32)> = if action == ACTION_INTERESTED {
        let features: Vec<_> = attribution
            .cited_positive
            .iter()
            .filter(|feature| feature.citeable && feature.recommendation_mean > 0.0)
            .take(3)
            .collect();
        let delta = if features.is_empty() {
            0.0
        } else {
            FEEDBACK_SIGNAL_WEIGHT_V1 / features.len() as f32
        };
        features
            .into_iter()
            .map(|feature| (feature.feature_key.clone(), delta))
            .collect()
    } else if matches!(reason, Some("wrong_connection") | Some("not_this_kind")) {
        target
            .map(|feature| vec![(feature.feature_key.clone(), -FEEDBACK_SIGNAL_WEIGHT_V1)])
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    requests
        .into_iter()
        .filter(|(feature_key, _)| !feature_key.is_empty())
        .map(|(feature_key, requested_delta)| {
            let pre_adjustment = current.get(&feature_key).copied().unwrap_or(0.0);
            let resulting_adjustment = (pre_adjustment + requested_delta)
                .clamp(-FEEDBACK_ADJUSTMENT_CAP, FEEDBACK_ADJUSTMENT_CAP);
            FeedbackAdjustment {
                feature_key,
                requested_delta,
                applied_delta: resulting_adjustment - pre_adjustment,
                pre_adjustment,
                resulting_adjustment,
            }
        })
        .collect()
}

fn upsert_feedback_state(
    db: &Database,
    tmdb_id: i64,
    action: &str,
    reason: Option<&str>,
    suppressed_until: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let key = content_key(tmdb_id);
    let created = get_one(db, &key)?
        .map(|row| row.created_at)
        .unwrap_or_else(|| now.to_string());
    db.conn()
        .execute(
            "INSERT INTO taste_feedback (content_key, tmdb_id, media_kind, action, reason, suppressed_until, created_at, updated_at) VALUES (?1, ?2, 'movie', ?3, ?4, ?5, ?6, ?7) ON CONFLICT(content_key) DO UPDATE SET action = excluded.action, reason = excluded.reason, suppressed_until = excluded.suppressed_until, updated_at = excluded.updated_at",
            params![key, tmdb_id, action, reason, suppressed_until, created, now],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn active_feedback_adjustments(db: &Database) -> Result<HashMap<String, f32>, String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT applied_adjustments_json FROM taste_feedback_events")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut totals = HashMap::new();
    for row in rows {
        let raw = row.map_err(|e| e.to_string())?;
        for adjustment in serde_json::from_str::<Vec<FeedbackAdjustment>>(&raw).unwrap_or_default() {
            let current = totals.entry(adjustment.feature_key).or_insert(0.0_f32);
            *current = (*current + adjustment.applied_delta)
                .clamp(-FEEDBACK_ADJUSTMENT_CAP, FEEDBACK_ADJUSTMENT_CAP);
        }
    }
    Ok(totals)
}

pub fn apply_feedback_adjustments(profile: &mut FeatureProfile, adjustments: &HashMap<String, f32>) {
    for affinity in &mut profile.affinities {
        affinity.feedback_adjustment = adjustments
            .get(&affinity.key.storage_key())
            .copied()
            .unwrap_or(0.0)
            .clamp(-FEEDBACK_ADJUSTMENT_CAP, FEEDBACK_ADJUSTMENT_CAP);
    }
}

pub fn mood_signature_for_candidate(candidate: &ScoredCandidate) -> TasteMoodSignature {
    TasteMoodSignature {
        modes: candidate.candidate.modes.clone(),
        thematic_keywords: candidate
            .matched_features
            .iter()
            .filter(|feature| {
                feature.cited
                    && feature.family == "keyword"
                    && matches!(keyword_strength(&feature.name), KeywordStrength::Strong | KeywordStrength::Thematic)
            })
            .map(|feature| feature.name.clone())
            .collect(),
    }
}

pub fn filter_mood_suppressed_candidates(
    db: &Database,
    candidates: &mut Vec<ScoredCandidate>,
) -> Result<(), String> {
    let suppressions = active_mood_suppressions(db)?;
    candidates.retain(|candidate| {
        let signature = mood_signature_for_candidate(candidate);
        !suppressions
            .iter()
            .any(|suppression| suppression.overlap_count(&signature) >= 2)
    });
    Ok(())
}

pub fn mood_signature_is_suppressed(
    db: &Database,
    signature: &TasteMoodSignature,
) -> Result<bool, String> {
    Ok(active_mood_suppressions(db)?
        .iter()
        .any(|suppression| suppression.overlap_count(signature) >= 2))
}

fn active_mood_suppressions(db: &Database) -> Result<Vec<TasteMoodSignature>, String> {
    let now = Utc::now().to_rfc3339();
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT exposure.snapshot_json FROM taste_feedback_events event JOIN taste_recommendation_exposures exposure ON exposure.id = event.exposure_id WHERE event.reason = 'not_in_the_mood' AND event.mood_scope = ?1 AND event.mood_fallback = 0 AND event.expires_at > ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![MOOD_SCOPE_KIND_RIGHT_NOW, now], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut signatures = Vec::new();
    for row in rows {
        let raw = row.map_err(|e| e.to_string())?;
        if let Ok(attribution) = serde_json::from_str::<TasteAttribution>(&raw) {
            if attribution.mood_signature.element_count() >= 2 {
                signatures.push(attribution.mood_signature);
            }
        }
    }
    Ok(signatures)
}

pub fn observation_summary(db: &Database) -> Result<TasteObservationSummary, String> {
    let scalar = |sql: &str| -> Result<i64, String> {
        db.conn()
            .query_row(sql, [], |row| row.get(0))
            .map_err(|e| e.to_string())
    };
    let feedback_events = scalar("SELECT COUNT(*) FROM taste_feedback_events")? as u32;
    let feedback_reasons = scalar("SELECT COUNT(DISTINCT reason) FROM taste_feedback_events WHERE reason IS NOT NULL")? as u32;
    let exposure_count = scalar("SELECT COUNT(*) FROM taste_recommendation_exposures")? as u32;
    let mood_fallbacks = scalar("SELECT COUNT(*) FROM taste_feedback_events WHERE mood_fallback = 1")? as u32;
    let later_outcomes = scalar(
        "SELECT COUNT(DISTINCT exposure.id) FROM taste_recommendation_exposures exposure JOIN movies movie ON movie.tmdb_id = exposure.tmdb_id JOIN user_movie_state state ON state.movie_id = movie.id WHERE EXISTS (SELECT 1 FROM viewings viewing WHERE viewing.source_movie_record_id = state.source_movie_record_id AND viewing.observed_at > exposure.created_at) OR EXISTS (SELECT 1 FROM rating_events rating WHERE rating.source_movie_record_id = state.source_movie_record_id AND rating.observed_at > exposure.created_at)",
    )? as u32;

    let mut exposure_by_feature = HashMap::<String, u32>::new();
    let mut stmt = db
        .conn()
        .prepare("SELECT feature_key, COUNT(*) FROM taste_exposure_features GROUP BY feature_key")
        .map_err(|e| e.to_string())?;
    for row in stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .map_err(|e| e.to_string())?
    {
        let (feature, count) = row.map_err(|e| e.to_string())?;
        exposure_by_feature.insert(feature, count.max(0) as u32);
    }
    let mut feedback_by_feature = HashMap::<String, u32>::new();
    let mut stmt = db
        .conn()
        .prepare("SELECT applied_adjustments_json FROM taste_feedback_events")
        .map_err(|e| e.to_string())?;
    for row in stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
    {
        for adjustment in serde_json::from_str::<Vec<FeedbackAdjustment>>(&row.map_err(|e| e.to_string())?)
            .unwrap_or_default()
        {
            *feedback_by_feature.entry(adjustment.feature_key).or_default() += 1;
        }
    }
    let mut feature_exposure: Vec<_> = exposure_by_feature
        .into_iter()
        .map(|(feature_key, exposures)| FeatureExposureMetric {
            feedback_events: feedback_by_feature.remove(&feature_key).unwrap_or(0),
            feature_key,
            exposures,
        })
        .collect();
    feature_exposure.sort_by(|a, b| {
        b.feedback_events
            .cmp(&a.feedback_events)
            .then_with(|| b.exposures.cmp(&a.exposures))
    });
    feature_exposure.truncate(24);

    let mood_signature_eligible = count_signature_eligible(db)?;
    Ok(TasteObservationSummary {
        feedback_events,
        later_outcomes,
        feedback_reasons,
        exposure_count,
        mood_signature_eligible,
        mood_fallbacks,
        phase_two_unlocked: feedback_events >= 100 && later_outcomes >= 30 && feedback_reasons >= 3,
        feature_exposure,
    })
}

fn count_signature_eligible(db: &Database) -> Result<u32, String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT snapshot_json FROM taste_recommendation_exposures")
        .map_err(|e| e.to_string())?;
    let mut count = 0;
    for row in stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
    {
        if serde_json::from_str::<TasteAttribution>(&row.map_err(|e| e.to_string())?)
            .map(|attribution| attribution.mood_signature.element_count() >= 2)
            .unwrap_or(false)
        {
            count += 1;
        }
    }
    Ok(count)
}

trait OptionalRow<T> {
    fn optional_row(self) -> Result<Option<T>, String>;
}

impl<T> OptionalRow<T> for Result<T, rusqlite::Error> {
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
    use crate::storage::db::Database;

    #[test]
    fn upsert_keeps_created_and_bumps_updated() {
        let db = Database::in_memory().expect("db");
        let first = set_feedback(&db, 11, ACTION_INTERESTED, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
        let second = set_feedback(&db, 11, ACTION_REJECTED, Some("not_this_kind".into())).unwrap();
        assert_eq!(first.created_at, second.created_at);
        assert_ne!(first.updated_at, second.updated_at);
        assert_eq!(second.action, ACTION_REJECTED);
    }

    #[test]
    fn rejects_unknown_action() {
        let db = Database::in_memory().expect("db");
        assert!(set_feedback(&db, 1, "love", None).is_err());
    }

    #[test]
    fn hide_ids_are_rejected_and_seen_only() {
        let db = Database::in_memory().expect("db");
        set_feedback(&db, 1, ACTION_INTERESTED, None).unwrap();
        set_feedback(&db, 2, ACTION_REJECTED, None).unwrap();
        set_feedback(&db, 3, ACTION_SEEN, None).unwrap();
        let hide = hide_ids(&list_feedback(&db).unwrap());
        assert!(!hide.contains(&1));
        assert!(hide.contains(&2));
        assert!(hide.contains(&3));
    }

    fn bridge(key: &str) -> MatchedFeatureView {
        MatchedFeatureView {
            feature_key: key.into(),
            name: key.into(),
            family: "keyword".into(),
            appearances: 3,
            recommendation_mean: 0.7,
            scoring_affinity: 0.4,
            confidence: 0.6,
            portability: 1.0,
            citeable: true,
            cited: true,
        }
    }

    fn exposure(id: &str) -> TasteAttribution {
        TasteAttribution {
            exposure_id: id.into(),
            run_id: "run".into(),
            tmdb_id: 11,
            title: "Example".into(),
            evidence_grade: "strong".into(),
            cited_positive: vec![bridge("Keyword:1:night"), bridge("Keyword:2:rain")],
            cited_negative: vec![],
            seed_films: vec!["Seed".into()],
            semantic_fit: 0.8,
            diversity_adjustment: 0.0,
            retrieval_source: "related".into(),
            ranking_rationale: vec!["Keyword bridge".into()],
            mood_signature: TasteMoodSignature {
                modes: vec!["atmosphere".into()],
                thematic_keywords: vec!["night".into()],
            },
            prior_candidate_exposures: 0,
            prior_feature_exposures: vec![],
        }
    }

    #[test]
    fn feedback_requires_visible_bridge_and_records_capped_adjustments() {
        let db = Database::in_memory().expect("db");
        let saved = record_exposure(&db, exposure("exposure")).expect("exposure");
        assert!(set_feedback_with_exposure(
            &db,
            TasteFeedbackRequest {
                tmdb_id: 11,
                action: ACTION_REJECTED.into(),
                reason: Some("wrong_connection".into()),
                exposure_id: Some(saved.exposure_id.clone()),
                target_feature_key: Some("missing".into()),
                mood_scope: None,
            },
        )
        .is_err());
        for _ in 0..3 {
            set_feedback_with_exposure(
                &db,
                TasteFeedbackRequest {
                    tmdb_id: 11,
                    action: ACTION_REJECTED.into(),
                    reason: Some("wrong_connection".into()),
                    exposure_id: Some(saved.exposure_id.clone()),
                    target_feature_key: Some("Keyword:1:night".into()),
                    mood_scope: None,
                },
            )
            .expect("feedback");
        }
        assert_eq!(
            active_feedback_adjustments(&db).unwrap()["Keyword:1:night"],
            -FEEDBACK_ADJUSTMENT_CAP
        );
    }

    #[test]
    fn sparse_mood_signature_falls_back_to_movie_only() {
        let db = Database::in_memory().expect("db");
        let mut attribution = exposure("sparse");
        attribution.mood_signature.thematic_keywords.clear();
        let saved = record_exposure(&db, attribution).unwrap();
        set_feedback_with_exposure(
            &db,
            TasteFeedbackRequest {
                tmdb_id: 11,
                action: ACTION_REJECTED.into(),
                reason: Some("not_in_the_mood".into()),
                exposure_id: Some(saved.exposure_id),
                target_feature_key: None,
                mood_scope: Some(MOOD_SCOPE_KIND_RIGHT_NOW.into()),
            },
        )
        .unwrap();
        assert!(!mood_signature_is_suppressed(
            &db,
            &TasteMoodSignature {
                modes: vec!["atmosphere".into()],
                thematic_keywords: vec!["night".into()],
            },
        )
        .unwrap());
        assert_eq!(observation_summary(&db).unwrap().mood_fallbacks, 1);
    }
}
