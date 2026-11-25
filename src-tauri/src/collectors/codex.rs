use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::collectors::{
    basename_from_path, collect_files_recursive, derive_tool_status, file_mtime_unix,
    parse_timestamp_to_unix,
};
use crate::schema::{AgentStatus, QuotaWindow, SessionSummary, ToolId, ToolSnapshot};

const RUNNING_STALE_SECONDS: i64 = 300;
const MAX_SCAN_FILES: usize = 12;

#[derive(Debug, Clone)]
pub struct CodexConfig {
    pub sessions_dir: PathBuf,
}

impl CodexConfig {
    pub fn default_from_home(home: &Path) -> Self {
        Self {
            sessions_dir: home.join(".codex").join("sessions"),
        }
    }
}

pub fn collect_codex(config: &CodexConfig, now_unix: i64) -> ToolSnapshot {
    let codex_root = config.sessions_dir.parent();
    let installed = config.sessions_dir.exists() || codex_root.map(|p| p.exists()).unwrap_or(false);

    if !config.sessions_dir.exists() {
        return ToolSnapshot {
            tool: ToolId::Codex,
            installed,
            hook_configured: None,
            status: if installed {
                AgentStatus::Idle
            } else {
                AgentStatus::Unavailable
            },
            quotas: vec![],
            active_sessions: vec![],
            last_updated_at: None,
        };
    }

    let mut files = Vec::new();
    collect_files_recursive(&config.sessions_dir, &mut files);
    files.retain(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
            .unwrap_or(false)
    });

    files.sort_by(|a, b| {
        let ma = file_mtime_unix(a).unwrap_or(0);
        let mb = file_mtime_unix(b).unwrap_or(0);
        mb.cmp(&ma)
    });

    let mut sessions = Vec::new();
    let mut latest_quotas: Option<(i64, Vec<QuotaWindow>)> = None;
    let mut latest_updated: Option<i64> = None;

    for path in files.into_iter().take(MAX_SCAN_FILES) {
        let mtime = file_mtime_unix(&path).unwrap_or(0);
        if let Ok(parsed) = parse_codex_rollout(&path, mtime, now_unix) {
            if let Some(ts) = parsed.last_updated_at {
                latest_updated = Some(latest_updated.map_or(ts, |prev| prev.max(ts)));
            }
            if !parsed.quotas.is_empty() {
                let q_ts = parsed.quota_timestamp.unwrap_or(mtime);
                let replace = latest_quotas
                    .as_ref()
                    .map(|(prev_ts, _)| q_ts >= *prev_ts)
                    .unwrap_or(true);
                if replace {
                    latest_quotas = Some((q_ts, parsed.quotas));
                }
            }
            if let Some(summary) = parsed.session {
                sessions.push(summary);
            }
        }
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions.truncate(5);

    let quotas = latest_quotas.map(|(_, q)| q).unwrap_or_default();
    let status = derive_tool_status(installed, &sessions, &quotas);

    ToolSnapshot {
        tool: ToolId::Codex,
        installed,
        hook_configured: None,
        status,
        quotas,
        active_sessions: sessions,
        last_updated_at: latest_updated,
    }
}

#[derive(Debug)]
struct ParsedRollout {
    session: Option<SessionSummary>,
    quotas: Vec<QuotaWindow>,
    quota_timestamp: Option<i64>,
    last_updated_at: Option<i64>,
}

fn parse_codex_rollout(path: &Path, fallback_mtime: i64, now_unix: i64) -> std::io::Result<ParsedRollout> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut session_id: Option<String> = None;
    let mut workspace: Option<String> = None;
    let mut last_ts: Option<i64> = None;
    let mut turn_active = false;
    let mut waiting_approval = false;
    let mut latest_rate_limits = None;

    for line_res in reader.lines() {
        let Ok(line) = line_res else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let event_ts = val
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_timestamp_to_unix);
        if let Some(ts) = event_ts {
            last_ts = Some(last_ts.map_or(ts, |prev| prev.max(ts)));
        }

        let top_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let payload = val.get("payload");

        match top_type {
            "session_meta" => {
                if let Some(p) = payload {
                    if session_id.is_none() {
                        session_id = p
                            .get("id")
                            .or_else(|| p.get("session_id"))
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    if workspace.is_none() {
                        workspace = p
                            .get("cwd")
                            .and_then(|v| v.as_str())
                            .and_then(basename_from_path);
                    }
                }
            }
            "event_msg" => {
                if let Some(p) = payload {
                    let p_type = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match p_type {
                        "task_started" | "turn_started" => {
                            turn_active = true;
                            waiting_approval = false;
                        }
                        "task_complete" | "turn_aborted" | "turn_completed" => {
                            turn_active = false;
                            waiting_approval = false;
                        }
                        "exec_approval_request"
                        | "apply_patch_approval_request"
                        | "request_user_input" => {
                            waiting_approval = true;
                        }
                        "token_count" => {
                            if let Some(rl) = p.get("rate_limits") {
                                let quotas = parse_codex_rate_limits(rl, now_unix);
                                if !quotas.is_empty() {
                                    let ts = event_ts.unwrap_or(fallback_mtime);
                                    latest_rate_limits = Some((ts, quotas));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let updated_at = last_ts.unwrap_or(fallback_mtime);
    let fresh = now_unix.saturating_sub(updated_at) < RUNNING_STALE_SECONDS;
    let session_status = if waiting_approval && fresh {
        AgentStatus::Waiting
    } else if turn_active && fresh {
        AgentStatus::Running
    } else {
        AgentStatus::Idle
    };

    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("codex-session");

    let sid = session_id.unwrap_or_else(|| file_stem.to_string());
    let title = workspace
        .as_ref()
        .map(|w| format!("Codex · {w}"))
        .unwrap_or_else(|| format!("Codex · {}", &sid.chars().take(8).collect::<String>()));

    let (quota_timestamp, quotas) = match latest_rate_limits {
        Some((ts, quotas)) => (Some(ts), quotas),
        None => (None, vec![]),
    };

    Ok(ParsedRollout {
        session: Some(SessionSummary {
            id: sid,
            title,
            workspace,
            status: session_status,
            updated_at,
        }),
        quotas,
        quota_timestamp,
        last_updated_at: Some(updated_at),
    })
}

fn parse_codex_rate_limits(rl: &Value, now_unix: i64) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    for key in ["primary", "secondary"] {
        let Some(obj) = rl.get(key) else { continue };
        if obj.is_null() {
            continue;
        }
        let Some(used) = obj.get("used_percent").and_then(|v| v.as_f64()) else {
            continue;
        };
        let window_minutes = obj
            .get("window_minutes")
            .and_then(|v| v.as_u64())
            .unwrap_or(if key == "primary" { 300 } else { 10_080 })
            as u32;
        let resets_at = obj.get("resets_at").and_then(|v| v.as_i64());

        // If the reset timestamp has already passed, treat the window as refreshed (0% used).
        let effective_used = match resets_at {
            Some(r) if r <= now_unix => 0.0,
            _ => used,
        };

        let (id, label) = if window_minutes <= 360 {
            ("five_hour", "5h")
        } else {
            ("weekly", "Week")
        };

        windows.push(QuotaWindow::from_used(
            id,
            label,
            effective_used,
            resets_at,
            window_minutes,
        ));
    }
    windows
}

pub fn parse_codex_fixture_for_test(path: &Path, now_unix: i64) -> ToolSnapshot {
    let parsed = parse_codex_rollout(path, now_unix, now_unix).expect("fixture should parse");
    let sessions = parsed.session.into_iter().collect::<Vec<_>>();
    let status = derive_tool_status(true, &sessions, &parsed.quotas);
    ToolSnapshot {
        tool: ToolId::Codex,
        installed: true,
        hook_configured: None,
        status,
        quotas: parsed.quotas,
        active_sessions: sessions,
        last_updated_at: parsed.last_updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_fixture_running_and_quotas() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("codex_rollout.jsonl");

        // 2026-11-18T12:01:30Z = 1795003290
        let now = parse_timestamp_to_unix("2026-11-18T12:01:30Z").unwrap();
        let snap = parse_codex_fixture_for_test(&fixture, now);

        assert_eq!(snap.tool, ToolId::Codex);
        assert_eq!(snap.status, AgentStatus::Running);
        assert_eq!(snap.quotas.len(), 2);
        assert_eq!(snap.quotas[0].label, "5h");
        assert!((snap.quotas[0].remaining_percent - 81.5).abs() < 0.01);
        assert_eq!(snap.quotas[1].label, "Week");
        assert!((snap.quotas[1].remaining_percent - 58.0).abs() < 0.01);
        assert_eq!(snap.active_sessions[0].workspace.as_deref(), Some("toki"));
    }

    #[test]
    fn ignores_trailing_empty_premium_rate_limits() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_rollout.jsonl");
        let now = parse_timestamp_to_unix("2026-11-18T12:01:30Z").unwrap();
        let parsed = parse_codex_rollout(&fixture, now, now).unwrap();

        assert_eq!(parsed.quotas.len(), 2);
        assert_eq!(
            parsed.quota_timestamp,
            parse_timestamp_to_unix("2026-11-18T12:00:12Z")
        );
    }

    #[test]
    fn expired_reset_timestamp_resets_used_percent_to_zero() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("codex_rollout.jsonl");

        // After resets_at (2000000000) and after stale running window
        let snap = parse_codex_fixture_for_test(&fixture, 2_000_000_010);
        assert_eq!(snap.status, AgentStatus::Idle);
        assert!((snap.quotas[0].remaining_percent - 100.0).abs() < 0.01);
    }

    #[test]
    fn missing_codex_dir_returns_unavailable_without_panic() {
        let cfg = CodexConfig {
            sessions_dir: PathBuf::from("/nonexistent/toki/codex/sessions"),
        };
        let snap = collect_codex(&cfg, 1_800_000_000);
        assert!(!snap.installed);
        assert_eq!(snap.status, AgentStatus::Unavailable);
        assert!(snap.quotas.is_empty());
    }
}
