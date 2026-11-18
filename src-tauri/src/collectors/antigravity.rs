use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::collectors::{
    basename_from_path, derive_tool_status, detect_hook_configured, parse_hook_file,
    parse_timestamp_to_unix,
};
use crate::schema::{AgentStatus, SessionSummary, ToolId, ToolSnapshot};

const RUNNING_STALE_SECONDS: i64 = 300;

#[derive(Debug, Clone)]
pub struct AntigravityConfig {
    pub cli_dir: PathBuf,
    pub desktop_dir: PathBuf,
    pub hook_cache_path: PathBuf,
}

impl AntigravityConfig {
    pub fn default_from_home(home: &Path) -> Self {
        Self {
            cli_dir: home.join(".gemini").join("antigravity-cli"),
            desktop_dir: home.join(".gemini").join("antigravity"),
            hook_cache_path: home.join(".toki").join("hooks").join("antigravity.json"),
        }
    }
}

pub fn collect_antigravity(config: &AntigravityConfig, now_unix: i64) -> ToolSnapshot {
    let installed = config.cli_dir.exists() || config.desktop_dir.exists();
    let settings_path = config.cli_dir.join("settings.json");
    let hook_configured = if installed {
        Some(detect_hook_configured(&settings_path))
    } else {
        None
    };

    if !installed {
        return ToolSnapshot {
            tool: ToolId::Antigravity,
            installed: false,
            hook_configured: None,
            status: AgentStatus::Unavailable,
            quotas: vec![],
            active_sessions: vec![],
            last_updated_at: None,
        };
    }

    let mut sessions = Vec::new();
    let mut seen_ids = HashSet::new();

    for dir in [&config.cli_dir, &config.desktop_dir] {
        let db_path = dir.join("conversation_summaries.db");
        if db_path.exists() {
            if let Ok(rows) = read_summaries_db(&db_path, now_unix) {
                for s in rows {
                    if seen_ids.insert(s.id.clone()) {
                        sessions.push(s);
                    }
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions.truncate(5);

    let last_updated_at = sessions.iter().map(|s| s.updated_at).max();
    let (hook_ts, quotas) = parse_hook_file(&config.hook_cache_path, now_unix);
    let combined_updated = match (last_updated_at, hook_ts) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };

    let status = derive_tool_status(installed, &sessions, &quotas);

    ToolSnapshot {
        tool: ToolId::Antigravity,
        installed,
        hook_configured,
        status,
        quotas,
        active_sessions: sessions,
        last_updated_at: combined_updated,
    }
}

pub fn read_summaries_db(db_path: &Path, now_unix: i64) -> rusqlite::Result<Vec<SessionSummary>> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let mut stmt = conn.prepare(
        "SELECT conversation_id, title, status, not_fully_idle, killed, last_modified_time, workspace_uris
         FROM conversation_summaries
         WHERE killed = 0
         ORDER BY last_modified_time DESC
         LIMIT 8",
    )?;

    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let status_str: String = row.get(2)?;
        let not_fully_idle: i64 = row.get(3).unwrap_or(0);
        let modified_str: String = row.get(5)?;
        let workspace_raw: String = row.get(6).unwrap_or_default();
        Ok((id, title, status_str, not_fully_idle, modified_str, workspace_raw))
    })?;

    let mut out = Vec::new();
    for item in rows.flatten() {
        let (id, title, status_str, not_fully_idle, modified_str, workspace_raw) = item;
        let updated_at = parse_timestamp_to_unix(&modified_str).unwrap_or(0);
        let fresh = now_unix.saturating_sub(updated_at) < RUNNING_STALE_SECONDS;
        let is_running =
            (status_str == "CASCADE_RUN_STATUS_RUNNING" || not_fully_idle != 0) && fresh;

        let workspace = extract_workspace_name(&workspace_raw);
        let clean_title = if title.trim().is_empty() {
            workspace
                .as_ref()
                .map(|w| format!("Agy · {w}"))
                .unwrap_or_else(|| "Antigravity Session".into())
        } else {
            title
        };

        out.push(SessionSummary {
            id,
            title: clean_title,
            workspace,
            status: if is_running {
                AgentStatus::Running
            } else {
                AgentStatus::Idle
            },
            updated_at,
        });
    }

    Ok(out)
}

fn extract_workspace_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('[') {
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                return basename_from_path(first.strip_prefix("file://").unwrap_or(first));
            }
        }
    }
    basename_from_path(trimmed.strip_prefix("file://").unwrap_or(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_sqlite_readonly_and_hook_fixture() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "toki-agy-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cli_dir = tmp_dir.join("antigravity-cli");
        std::fs::create_dir_all(&cli_dir).unwrap();
        let db_path = cli_dir.join("conversation_summaries.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE conversation_summaries (
                    conversation_id TEXT PRIMARY KEY,
                    title TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL DEFAULT '',
                    not_fully_idle INTEGER NOT NULL DEFAULT 0,
                    killed INTEGER NOT NULL DEFAULT 0,
                    last_modified_time TEXT NOT NULL,
                    workspace_uris TEXT NOT NULL DEFAULT ''
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO conversation_summaries VALUES
                 ('conv-1', 'Implement Phase 3', 'CASCADE_RUN_STATUS_RUNNING', 1, 0, '2026-11-18 12:55:00+00:00', 'file:///Users/demo/Project/toki'),
                 ('conv-2', 'Killed session', 'CASCADE_RUN_STATUS_RUNNING', 1, 1, '2026-11-18 12:55:10+00:00', '')",
                [],
            )
            .unwrap();
        }

        let hook_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("antigravity_hook.json");

        let cfg = AntigravityConfig {
            cli_dir,
            desktop_dir: tmp_dir.join("nonexistent-desktop"),
            hook_cache_path: hook_fixture,
        };

        let now = parse_timestamp_to_unix("2026-11-18 12:55:20+00:00").unwrap();
        let snap = collect_antigravity(&cfg, now);

        assert!(snap.installed);
        assert_eq!(snap.hook_configured, Some(false));
        assert_eq!(snap.status, AgentStatus::Running);
        assert_eq!(snap.active_sessions.len(), 1);
        assert_eq!(snap.active_sessions[0].title, "Implement Phase 3");
        assert_eq!(snap.active_sessions[0].workspace.as_deref(), Some("toki"));
        assert_eq!(snap.quotas.len(), 3);
        assert_eq!(snap.quotas[0].label, "5h");
        assert!((snap.quotas[0].remaining_percent - 88.3312).abs() < 0.05);
        assert_eq!(snap.quotas[1].label, "Week");
        assert!((snap.quotas[1].remaining_percent - 72.54447).abs() < 0.05);
        assert_eq!(snap.quotas[2].label, "Argon");
        assert!((snap.quotas[2].remaining_percent - 56.35487).abs() < 0.05);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
