use crate::storage::db::Database;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};

pub const ACTION_INTERESTED: &str = "interested";
pub const ACTION_REJECTED: &str = "rejected";
pub const ACTION_SEEN: &str = "seen";

const ALLOWED_REASONS: &[&str] = &[
    "already_seen_disliked",
    "not_this_kind",
    "wrong_connection",
    "not_in_the_mood",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteFeedback {
    pub content_key: String,
    pub tmdb_id: i64,
    pub media_kind: String,
    pub action: String,
    pub reason: Option<String>,
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
            INSERT INTO taste_feedback (content_key, tmdb_id, media_kind, action, reason, created_at, updated_at)
            VALUES (?1, ?2, 'movie', ?3, ?4, ?5, ?6)
            ON CONFLICT(content_key) DO UPDATE SET
              action = excluded.action,
              reason = excluded.reason,
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
            "SELECT content_key, tmdb_id, media_kind, action, reason, created_at, updated_at
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
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn get_one(db: &Database, key: &str) -> Result<Option<TasteFeedback>, String> {
    db.conn()
        .query_row(
            "SELECT content_key, tmdb_id, media_kind, action, reason, created_at, updated_at
             FROM taste_feedback WHERE content_key = ?1",
            params![key],
            |row| {
                Ok(TasteFeedback {
                    content_key: row.get(0)?,
                    tmdb_id: row.get(1)?,
                    media_kind: row.get(2)?,
                    action: row.get(3)?,
                    reason: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
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
}
