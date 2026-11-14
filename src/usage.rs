//! Provider-reported token metadata only. No prompts, responses, paths, keys or prices.
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub id: String,
    pub at: i64,
    pub model: String,
    pub provider: String,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub reasoning: u64,
    pub duration_ms: u64,
}
#[derive(Clone, Serialize, Debug)]
pub struct Day {
    pub day: String,
    pub model: String,
    pub provider: String,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub calls: u64,
    pub duration_ms: u64,
}

pub fn reported(value: &Value, model: &str, provider: &str, duration_ms: u64) -> Option<Usage> {
    let usage = value.get("usage")?;
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))?
        .as_u64()?;
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))?
        .as_u64()?;
    // SQLite uses signed integers. Reject impossible counts instead of wrapping them.
    if input > i64::MAX as u64 || output > i64::MAX as u64 {
        return None;
    }
    let cached = usage
        .pointer("/input_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/prompt_tokens_details/cached_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(input);
    let reasoning = usage
        .pointer("/output_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(output);
    let label = |s: &str| s.chars().filter(|c| !c.is_control()).take(128).collect();
    Some(Usage {
        id: uuid::Uuid::new_v4().to_string(),
        at: chrono::Utc::now().timestamp(),
        model: label(model),
        provider: label(provider),
        input,
        output,
        cached,
        reasoning,
        duration_ms: duration_ms.min(i64::MAX as u64),
    })
}
fn repo_key(repo: &Path) -> String {
    format!("{:x}", Sha256::digest(repo.to_string_lossy().as_bytes()))
}
fn schema(conn: &Connection) -> Result<(), String> {
    conn.busy_timeout(Duration::from_millis(250))
        .map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS provider_usage (id TEXT PRIMARY KEY, repo TEXT NOT NULL, at INTEGER NOT NULL, model TEXT NOT NULL, provider TEXT NOT NULL, input INTEGER NOT NULL, output INTEGER NOT NULL, cached INTEGER NOT NULL, reasoning INTEGER NOT NULL, duration_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS usage_repo_at ON provider_usage(repo,at);").map_err(|e|e.to_string())
}
fn insert(conn: &Connection, repo: &Path, u: &Usage) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO provider_usage VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            u.id,
            repo_key(repo),
            u.at,
            u.model,
            u.provider,
            u.input as i64,
            u.output as i64,
            u.cached as i64,
            u.reasoning as i64,
            u.duration_ms as i64
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn record(repo: &Path, usage: &Usage) -> Result<(), String> {
    let path = crate::persistence::config_dir().join("usage.sqlite3");
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Usage database must not be a symlink".into());
    }
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    schema(&conn)?;
    insert(&conn, repo, usage)
}
fn count(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}
fn query(conn: &Connection, repo: &Path) -> Result<Vec<Day>, String> {
    conn.busy_timeout(Duration::from_millis(250))
        .map_err(|e| e.to_string())?;
    let mut stmt=conn.prepare("SELECT date(at,'unixepoch'),model,provider,SUM(input),SUM(output),SUM(cached),COUNT(*),SUM(duration_ms) FROM provider_usage WHERE repo=?1 AND at>=strftime('%s','now','-365 days') GROUP BY 1,2,3 ORDER BY 1 DESC LIMIT 2000").map_err(|e|e.to_string())?;
    let rows = stmt
        .query_map([repo_key(repo)], |r| {
            Ok(Day {
                day: r.get(0)?,
                model: r.get(1)?,
                provider: r.get(2)?,
                input: count(r, 3)?,
                output: count(r, 4)?,
                cached: count(r, 5)?,
                calls: count(r, 6)?,
                duration_ms: count(r, 7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
pub fn history(repo: &Path) -> Result<Vec<Day>, String> {
    let path = crate::persistence::config_dir().join("usage.sqlite3");
    if !path.exists() {
        return Ok(Vec::new());
    }
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Usage database must not be a symlink".into());
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    query(&conn, repo)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_valid_reported_usage_is_counted_and_details_are_subsets() {
        assert!(reported(&serde_json::json!({}), "test", "local", 1).is_none());
        assert!(reported(
            &serde_json::json!({"usage":{"input_tokens":-1,"output_tokens":2}}),
            "test",
            "local",
            1
        )
        .is_none());
        let usage=reported(&serde_json::json!({"usage":{"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":500},"output_tokens_details":{"reasoning_tokens":5}}}),"test","local",7).unwrap();
        assert_eq!(usage.cached, 100);
        assert_eq!(usage.reasoning, 5);
    }
    #[test]
    fn ledger_survives_restart_deduplicates_and_isolates_workspaces() {
        let path = std::env::temp_dir().join(format!("osmogrep-usage-{}.db", uuid::Uuid::new_v4()));
        let usage = reported(
            &serde_json::json!({"usage":{"input_tokens":100,"output_tokens":20}}),
            "test",
            "local",
            7,
        )
        .unwrap();
        {
            let conn = Connection::open(&path).unwrap();
            schema(&conn).unwrap();
            insert(&conn, Path::new("repo-a"), &usage).unwrap();
            insert(&conn, Path::new("repo-a"), &usage).unwrap();
        }
        {
            let conn = Connection::open(&path).unwrap();
            let days = query(&conn, Path::new("repo-a")).unwrap();
            assert_eq!(days.len(), 1);
            assert_eq!(days[0].calls, 1);
            assert_eq!(days[0].input, 100);
            assert!(query(&conn, Path::new("repo-b")).unwrap().is_empty());
        }
        std::fs::remove_file(path).unwrap();
    }
}
