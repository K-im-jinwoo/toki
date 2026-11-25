use std::path::{Path, PathBuf};

use crate::collectors::{
    basename_from_path, collect_files_recursive, derive_tool_status, detect_hook_configured,
    file_mtime_unix, parse_hook_file,
};
use crate::schema::{AgentStatus, SessionSummary, ToolId, ToolSnapshot};

const ACTIVE_MTIME_SECONDS: i64 = 45;

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    pub claude_dir: PathBuf,
    pub hook_cache_path: PathBuf,
}

impl ClaudeConfig {
    pub fn default_from_home(home: &Path) -> Self {
        Self {
            claude_dir: home.join(".claude"),
            hook_cache_path: home.join(".toki").join("hooks").join("claude.json"),
        }
    }
}

pub fn collect_claude(config: &ClaudeConfig, now_unix: i64) -> ToolSnapshot {
    let installed = config.claude_dir.exists();
    if !installed {
        return ToolSnapshot {
            tool: ToolId::Claude,
            installed: false,
            hook_configured: None,
            status: AgentStatus::Unavailable,
            quotas: vec![],
            active_sessions: vec![],
            last_updated_at: None,
        };
    }

    let settings_path = config.claude_dir.join("settings.json");
    let hook_configured = Some(detect_hook_configured(&settings_path));

    let projects_dir = config.claude_dir.join("projects");
    let mut sessions = Vec::new();
    if projects_dir.exists() {
        let mut files = Vec::new();
        collect_files_recursive(&projects_dir, &mut files);
        files.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl"));
        files.sort_by(|a, b| {
            let ma = file_mtime_unix(a).unwrap_or(0);
            let mb = file_mtime_unix(b).unwrap_or(0);
            mb.cmp(&ma)
        });

        for path in files.into_iter().take(5) {
            let mtime = file_mtime_unix(&path).unwrap_or(0);
            let active = now_unix.saturating_sub(mtime) < ACTIVE_MTIME_SECONDS;
            let project_folder = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .and_then(basename_from_path);
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("claude-session")
                .to_string();
            let title = project_folder
                .as_ref()
                .map(|w| format!("Claude · {w}"))
                .unwrap_or_else(|| "Claude Code".into());

            sessions.push(SessionSummary {
                id: sid,
                title,
                workspace: project_folder,
                status: if active {
                    AgentStatus::Running
                } else {
                    AgentStatus::Idle
                },
                updated_at: mtime,
            });
        }
    }

    let (hook_ts, quotas) = parse_hook_file(&config.hook_cache_path, now_unix);
    let session_ts = sessions.iter().map(|s| s.updated_at).max();
    let last_updated_at = match (session_ts, hook_ts) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };

    let status = derive_tool_status(installed, &sessions, &quotas);

    ToolSnapshot {
        tool: ToolId::Claude,
        installed,
        hook_configured,
        status,
        quotas,
        active_sessions: sessions,
        last_updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_hook_fixture_and_handles_missing_projects() {
        let hook_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("claude_hook.json");

        let tmp_claude = std::env::temp_dir().join("toki-claude-test-dir");
        std::fs::create_dir_all(&tmp_claude).unwrap();

        let cfg = ClaudeConfig {
            claude_dir: tmp_claude.clone(),
            hook_cache_path: hook_fixture,
        };
        let snap = collect_claude(&cfg, 1_800_000_010);

        assert!(snap.installed);
        assert_eq!(snap.hook_configured, Some(false));
        assert_eq!(snap.status, AgentStatus::Idle);
        assert_eq!(snap.quotas.len(), 3);
        assert_eq!(snap.quotas[0].label, "5h");
        assert!((snap.quotas[0].remaining_percent - 72.5).abs() < 0.01);
        assert_eq!(snap.quotas[1].label, "Week");
        assert!((snap.quotas[1].remaining_percent - 39.0).abs() < 0.01);

        assert!(snap.quotas[0].group.is_none());
        assert!(snap.quotas[1].group.is_none());
        assert_eq!(snap.quotas[2].id, "seven_day_opus");
        assert_eq!(snap.quotas[2].group.as_deref(), Some("Opus"));
        assert_eq!(snap.quotas[2].label, "Week");
        assert_eq!(snap.quotas[2].remaining_percent, 65.0);

        let _ = std::fs::remove_dir_all(&tmp_claude);
    }

    #[test]
    fn uninstalled_claude_returns_unavailable() {
        let cfg = ClaudeConfig {
            claude_dir: PathBuf::from("/nonexistent/toki/claude"),
            hook_cache_path: PathBuf::from("/nonexistent/toki/claude.json"),
        };
        let snap = collect_claude(&cfg, 1_800_000_000);
        assert!(!snap.installed);
        assert_eq!(snap.hook_configured, None);
        assert_eq!(snap.status, AgentStatus::Unavailable);
    }
}
