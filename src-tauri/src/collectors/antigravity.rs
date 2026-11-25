use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::collectors::{
    basename_from_path, derive_tool_status, detect_hook_configured, parse_hook_quotas,
    parse_timestamp_to_unix, read_hook_payload,
};
use crate::schema::{AgentStatus, SessionSummary, ToolId, ToolSnapshot};

const RUNNING_STALE_SECONDS: i64 = 300;
const HOOK_ACTIVE_STALE_SECONDS: i64 = 90;
const HOOK_CLOCK_SKEW_SECONDS: i64 = 5;

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

    let (hook_ts, payload) = read_hook_payload(&config.hook_cache_path).unwrap_or_default();
    let conversation_id = hook_string(payload.get("conversation_id"))
        .or_else(|| hook_string(payload.get("session_id")));
    let quotas = parse_hook_quotas(&payload, hook_ts, now_unix);
    let mut sessions_by_id = HashMap::new();

    for dir in [&config.cli_dir, &config.desktop_dir] {
        let db_path = dir.join("conversation_summaries.db");
        if let Ok(rows) = read_summaries_db(&db_path, now_unix, conversation_id) {
            for (session, killed) in rows {
                match sessions_by_id.entry(session.id.clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert((session, killed));
                    }
                    Entry::Occupied(mut entry) => {
                        let (previous, previous_killed) = entry.get();
                        if (session.updated_at, killed) > (previous.updated_at, *previous_killed) {
                            entry.insert((session, killed));
                        }
                    }
                }
            }
        }
    }

    let hook_conversation_killed = conversation_id
        .and_then(|id| sessions_by_id.get(id))
        .is_some_and(|(_, killed)| *killed);
    let mut sessions: Vec<_> = sessions_by_id
        .into_values()
        .filter_map(|(session, killed)| (!killed).then_some(session))
        .collect();
    if let Some(hook_ts) = hook_ts.filter(|_| !hook_conversation_killed) {
        merge_hook_session(&mut sessions, &payload, conversation_id, hook_ts, now_unix);
    }
    sessions.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    sessions.truncate(5);

    let combined_updated = sessions.first().map(|s| s.updated_at).max(hook_ts);
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

fn hook_string(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn merge_hook_session(
    sessions: &mut Vec<SessionSummary>,
    payload: &Value,
    conversation_id: Option<&str>,
    hook_ts: i64,
    now_unix: i64,
) {
    let Some(state) = hook_string(payload.get("agent_state")) else {
        return;
    };
    let status = match state.to_ascii_lowercase().as_str() {
        "working" | "running" | "busy" | "thinking" | "streaming" => AgentStatus::Running,
        "waiting" | "blocked" | "needs_approval" | "approval" | "waiting_for_input" => {
            AgentStatus::Waiting
        }
        "idle" | "done" | "completed" | "ready" => AgentStatus::Idle,
        _ => return,
    };
    let active = matches!(status, AgentStatus::Running | AgentStatus::Waiting);
    // The cache can be updated after the collector captures now_unix.
    let hook_age = now_unix.saturating_sub(hook_ts);
    if active && !(-HOOK_CLOCK_SKEW_SECONDS..HOOK_ACTIVE_STALE_SECONDS).contains(&hook_age) {
        return;
    }

    let existing = if let Some(id) = conversation_id {
        sessions.iter_mut().find(|session| session.id == id)
    } else if active
        && sessions
            .iter()
            .all(|session| session.status == AgentStatus::Idle)
    {
        sessions.iter_mut().max_by(|a, b| {
            a.updated_at
                .cmp(&b.updated_at)
                .then_with(|| b.id.cmp(&a.id))
        })
    } else {
        return;
    };

    if let Some(session) = existing {
        if hook_ts >= session.updated_at {
            session.status = status;
            session.updated_at = hook_ts;
        }
    } else if active {
        let workspace = hook_string(payload.get("cwd"))
            .or_else(|| hook_string(payload.pointer("/workspace/current_dir")))
            .and_then(basename_from_path);
        sessions.push(SessionSummary {
            id: conversation_id.unwrap_or("antigravity-hook").into(),
            title: default_session_title(workspace.as_deref()),
            workspace,
            status,
            updated_at: hook_ts,
        });
    }
}

fn default_session_title(workspace: Option<&str>) -> String {
    workspace.map_or_else(|| "Antigravity Session".into(), |w| format!("Agy · {w}"))
}

fn read_summaries_db(
    db_path: &Path,
    now_unix: i64,
    hook_id: Option<&str>,
) -> rusqlite::Result<Vec<(SessionSummary, bool)>> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // Look up the hook conversation separately, including killed rows so a stale
    // working hook cannot resurrect a conversation the DB already closed.
    let mut stmt = conn.prepare(
        "WITH recent AS (
             SELECT conversation_id, title, status, not_fully_idle, last_modified_time, workspace_uris, killed
             FROM conversation_summaries
             WHERE killed = 0
             ORDER BY last_modified_time DESC, conversation_id
             LIMIT 8
         )
         SELECT * FROM recent
         UNION ALL
         SELECT conversation_id, title, status, not_fully_idle, last_modified_time, workspace_uris, killed
         FROM conversation_summaries
         WHERE conversation_id = ?1
           AND conversation_id NOT IN (SELECT conversation_id FROM recent)",
    )?;

    let rows = stmt.query_map([hook_id], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let status_str: String = row.get(2)?;
        let not_fully_idle: i64 = row.get(3).unwrap_or(0);
        let modified_str: String = row.get(4)?;
        let workspace_raw: String = row.get(5).unwrap_or_default();
        let killed: bool = row.get(6)?;
        Ok((
            id,
            title,
            status_str,
            not_fully_idle,
            modified_str,
            workspace_raw,
            killed,
        ))
    })?;

    let mut out = Vec::new();
    for item in rows.flatten() {
        let (id, title, status_str, not_fully_idle, modified_str, workspace_raw, killed) = item;
        let updated_at = parse_timestamp_to_unix(&modified_str).unwrap_or(0);
        let fresh = now_unix.saturating_sub(updated_at) < RUNNING_STALE_SECONDS;
        let status = if !fresh {
            AgentStatus::Idle
        } else if matches!(
            status_str.as_str(),
            "CASCADE_RUN_STATUS_RUNNING" | "CASCADE_RUN_STATUS_BUSY"
        ) || not_fully_idle != 0
        {
            AgentStatus::Running
        } else if status_str.contains("WAITING") || status_str.contains("APPROVAL") {
            AgentStatus::Waiting
        } else {
            AgentStatus::Idle
        };

        let workspace = extract_workspace_name(&workspace_raw);
        let clean_title = if title.trim().is_empty() {
            default_session_title(workspace.as_deref())
        } else {
            title
        };

        out.push((
            SessionSummary {
                id,
                title: clean_title,
                workspace,
                status,
                updated_at,
            },
            killed,
        ));
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("toki-agy-test-{pid}-{created_at}-{id}"))
    }

    #[test]
    fn reads_sqlite_readonly_and_hook_fixture() {
        let tmp_dir = test_temp_dir();
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
        assert_eq!(snap.quotas[2].label, "Week");
        assert_eq!(snap.quotas[2].group.as_deref(), Some("Argon"));
        assert!((snap.quotas[2].remaining_percent - 56.35487).abs() < 0.05);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    const HOOK_TIME: i64 = 1_800_000_000;

    fn test_config() -> AntigravityConfig {
        let root = test_temp_dir();
        let config = AntigravityConfig {
            cli_dir: root.join("cli"),
            desktop_dir: root.join("desktop"),
            hook_cache_path: root.join("hook.json"),
        };
        std::fs::create_dir_all(&config.cli_dir).unwrap();
        let conn = Connection::open(config.cli_dir.join("conversation_summaries.db")).unwrap();
        conn.execute_batch(include_str!(
            "../../tests/fixtures/antigravity_summaries.sql"
        ))
        .unwrap();
        config
    }

    fn collect_with_hook(hook: serde_json::Value, now: i64) -> ToolSnapshot {
        let config = test_config();
        std::fs::write(&config.hook_cache_path, hook.to_string()).unwrap();
        let snapshot = collect_antigravity(&config, now);
        std::fs::remove_dir_all(config.cli_dir.parent().unwrap()).unwrap();
        snapshot
    }

    fn session<'a>(snapshot: &'a ToolSnapshot, id: &str) -> &'a SessionSummary {
        snapshot
            .active_sessions
            .iter()
            .find(|s| s.id == id)
            .unwrap()
    }

    #[test]
    fn newer_working_hook_promotes_matching_idle_session() {
        let snapshot = collect_with_hook(
            serde_json::json!({
                "updated_at": HOOK_TIME,
                "agent_state": "  WORKING  ",
                "conversation_id": "idle"
            }),
            HOOK_TIME + 10,
        );
        let updated = session(&snapshot, "idle");
        assert_eq!(updated.status, AgentStatus::Running);
        assert_eq!(updated.updated_at, HOOK_TIME);
        assert_eq!(updated.title, "Idle conversation");
        assert_eq!(snapshot.active_sessions[0].id, "idle");
    }

    #[test]
    fn newer_idle_hook_only_demotes_its_own_conversation() {
        let snapshot = collect_with_hook(
            serde_json::json!({
                "updated_at": HOOK_TIME,
                "agent_state": "idle",
                "conversation_id": "running"
            }),
            HOOK_TIME + 10,
        );
        assert_eq!(session(&snapshot, "running").status, AgentStatus::Idle);
        assert_eq!(session(&snapshot, "running").updated_at, HOOK_TIME);
        assert_eq!(session(&snapshot, "busy").status, AgentStatus::Running);
    }

    #[test]
    fn fresh_hook_adds_new_session_before_database_flush() {
        let hook = serde_json::from_str(include_str!(
            "../../tests/fixtures/antigravity_hook_multi_pool.json"
        ))
        .unwrap();
        let snapshot = collect_with_hook(hook, HOOK_TIME + 89);
        let new = session(&snapshot, "new-conv");
        assert_eq!(new.status, AgentStatus::Running);
        assert_eq!(new.workspace.as_deref(), Some("toki"));
        assert_eq!(new.title, "Agy · toki");
        assert_eq!(snapshot.active_sessions[0].id, "new-conv");
    }

    #[test]
    fn active_hook_tolerates_small_clock_skew() {
        for (offset, state, expected) in [
            (1, "working", AgentStatus::Running),
            (5, "waiting", AgentStatus::Waiting),
            (6, "working", AgentStatus::Idle),
        ] {
            let snapshot = collect_with_hook(
                serde_json::json!({
                    "updated_at": HOOK_TIME + offset,
                    "agent_state": state,
                    "conversation_id": "idle"
                }),
                HOOK_TIME,
            );
            assert_eq!(session(&snapshot, "idle").status, expected, "offset {offset}");
        }
    }

    #[test]
    fn stale_working_hook_cannot_promote_or_create_a_session() {
        for (id, age) in [("idle", 90), ("new-conv", 91)] {
            let snapshot = collect_with_hook(
                serde_json::json!({
                    "updated_at": HOOK_TIME,
                    "agent_state": "working",
                    "conversation_id": id
                }),
                HOOK_TIME + age,
            );
            assert_eq!(session(&snapshot, "idle").status, AgentStatus::Idle);
            assert_eq!(snapshot.active_sessions.len(), 4);
        }
    }

    #[test]
    fn working_hook_cannot_revive_a_killed_conversation() {
        let snapshot = collect_with_hook(
            serde_json::json!({
                "updated_at": HOOK_TIME - 15,
                "agent_state": "working",
                "conversation_id": "killed"
            }),
            HOOK_TIME,
        );
        assert!(snapshot.active_sessions.iter().all(|s| s.id != "killed"));
    }

    #[test]
    fn sqlite_reads_fresh_busy_and_waiting_states() {
        let snapshot = collect_with_hook(serde_json::json!({}), HOOK_TIME);
        assert_eq!(session(&snapshot, "busy").status, AgentStatus::Running);
        assert_eq!(session(&snapshot, "waiting").status, AgentStatus::Waiting);
    }

    #[test]
    fn older_idle_hook_cannot_override_a_newer_running_row() {
        let snapshot = collect_with_hook(
            serde_json::json!({
                "updated_at": HOOK_TIME - 21,
                "agent_state": "idle",
                "conversation_id": "running"
            }),
            HOOK_TIME,
        );
        assert_eq!(session(&snapshot, "running").status, AgentStatus::Running);
        assert_eq!(session(&snapshot, "running").updated_at, HOOK_TIME - 20);
    }

    #[test]
    fn legacy_hook_with_no_id_promotes_latest_session_only_if_none_are_active() {
        let payload = |updated_at| {
            serde_json::json!({
                "updated_at": updated_at,
                "agent_state": "waiting_for_input",
                "workspace": { "current_dir": "/Users/demo/Project/toki" }
            })
        };
        let fresh_db = collect_with_hook(payload(HOOK_TIME), HOOK_TIME);
        assert_eq!(session(&fresh_db, "running").status, AgentStatus::Running);
        assert!(fresh_db
            .active_sessions
            .iter()
            .all(|s| s.updated_at < HOOK_TIME));

        let stale_db = collect_with_hook(payload(HOOK_TIME + 300), HOOK_TIME + 300);
        assert_eq!(stale_db.active_sessions.len(), 4);
        assert_eq!(stale_db.active_sessions[0].status, AgentStatus::Waiting);
        assert_eq!(stale_db.active_sessions[0].updated_at, HOOK_TIME + 300);
    }
}
