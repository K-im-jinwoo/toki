use crate::schema::{AgentStatus, MascotState, ToolSnapshot, UsageSnapshot};

pub const IDLE_TO_SLEEP_SECONDS: i64 = 600;
pub const EXHAUSTED_THRESHOLD_PERCENT: f64 = 5.0;

/// Evaluates the combined mascot state from all tool snapshots using strict priority:
/// Alert > Sprint > Run > Exhausted > Idle > Sleep.
pub fn compute_mascot_state(tools: &[ToolSnapshot], now_unix: i64) -> MascotState {
    let any_waiting = tools.iter().any(|t| {
        t.installed
            && (t.status == AgentStatus::Waiting
                || t.active_sessions
                    .iter()
                    .any(|s| s.status == AgentStatus::Waiting))
    });
    if any_waiting {
        return MascotState::Alert;
    }

    let running_count: usize = tools
        .iter()
        .filter(|t| t.installed)
        .map(|t| {
            let session_running = t
                .active_sessions
                .iter()
                .filter(|s| s.status == AgentStatus::Running)
                .count();
            if session_running > 0 {
                session_running
            } else if t.status == AgentStatus::Running {
                1
            } else {
                0
            }
        })
        .sum();

    if running_count >= 2 {
        return MascotState::Sprint;
    }
    if running_count == 1 {
        return MascotState::Run;
    }

    let any_exhausted = tools.iter().any(|t| {
        t.installed
            && t.quotas
                .iter()
                .any(|w| w.remaining_percent <= EXHAUSTED_THRESHOLD_PERCENT)
    });
    if any_exhausted {
        return MascotState::Exhausted;
    }

    let recently_active = tools.iter().any(|t| {
        if !t.installed {
            return false;
        }
        let tool_recent = t
            .last_updated_at
            .map(|ts| now_unix.saturating_sub(ts) < IDLE_TO_SLEEP_SECONDS)
            .unwrap_or(false);
        let session_recent = t
            .active_sessions
            .iter()
            .any(|s| now_unix.saturating_sub(s.updated_at) < IDLE_TO_SLEEP_SECONDS);
        tool_recent || session_recent
    });

    if recently_active {
        MascotState::Idle
    } else {
        MascotState::Sleep
    }
}

/// Builds a complete `UsageSnapshot` from normalized per-tool snapshots.
pub fn build_usage_snapshot(tools: Vec<ToolSnapshot>, now_unix: i64) -> UsageSnapshot {
    let overall_state = compute_mascot_state(&tools, now_unix);
    UsageSnapshot {
        overall_state,
        tools,
        generated_at: now_unix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{QuotaWindow, SessionSummary, ToolId};

    fn base_tool(tool: ToolId) -> ToolSnapshot {
        ToolSnapshot {
            tool,
            installed: true,
            hook_configured: None,
            status: AgentStatus::Idle,
            quotas: vec![
                QuotaWindow::from_used("five_hour", "5h", 20.0, Some(2_000), 300),
                QuotaWindow::from_used("weekly", "Week", 10.0, Some(10_000), 10_080),
            ],
            active_sessions: vec![],
            last_updated_at: Some(1_000),
        }
    }

    #[test]
    fn alert_takes_highest_priority_over_sprint_and_exhausted() {
        let mut codex = base_tool(ToolId::Codex);
        codex.status = AgentStatus::Running;
        codex.quotas = vec![QuotaWindow::from_used("five_hour", "5h", 99.0, None, 300)];

        let mut agy = base_tool(ToolId::Antigravity);
        agy.status = AgentStatus::Running;

        let mut claude = base_tool(ToolId::Claude);
        claude.status = AgentStatus::Waiting;

        assert_eq!(
            compute_mascot_state(&[codex, agy, claude], 1_100),
            MascotState::Alert
        );
    }

    #[test]
    fn session_waiting_also_triggers_alert() {
        let mut codex = base_tool(ToolId::Codex);
        codex.status = AgentStatus::Running;
        codex.active_sessions.push(SessionSummary {
            id: "s1".into(),
            title: "Approve edit".into(),
            workspace: None,
            status: AgentStatus::Waiting,
            updated_at: 1_050,
        });

        assert_eq!(compute_mascot_state(&[codex], 1_100), MascotState::Alert);
    }

    #[test]
    fn sprint_when_two_or_more_running_sessions() {
        let mut codex = base_tool(ToolId::Codex);
        codex.status = AgentStatus::Running;
        codex.active_sessions = vec![
            SessionSummary {
                id: "s1".into(),
                title: "Task A".into(),
                workspace: None,
                status: AgentStatus::Running,
                updated_at: 1_090,
            },
            SessionSummary {
                id: "s2".into(),
                title: "Task B".into(),
                workspace: None,
                status: AgentStatus::Running,
                updated_at: 1_095,
            },
        ];

        assert_eq!(compute_mascot_state(&[codex], 1_100), MascotState::Sprint);
    }

    #[test]
    fn run_when_single_tool_running() {
        let mut codex = base_tool(ToolId::Codex);
        codex.status = AgentStatus::Running;
        let agy = base_tool(ToolId::Antigravity);

        assert_eq!(compute_mascot_state(&[codex, agy], 1_100), MascotState::Run);
    }

    #[test]
    fn exhausted_at_five_percent_boundary_for_any_quota_bucket() {
        let mut agy = base_tool(ToolId::Antigravity);
        agy.quotas = vec![
            QuotaWindow::from_used("five_hour", "5h", 10.0, None, 300),
            QuotaWindow::from_used("weekly", "Week", 20.0, None, 10_080),
            QuotaWindow::from_used("argon", "Argon", 95.0, None, 10_080), // 5.0% left
        ];
        assert_eq!(
            compute_mascot_state(&[agy.clone()], 1_100),
            MascotState::Exhausted
        );

        agy.quotas[2] = QuotaWindow::from_used("argon", "Argon", 94.9, None, 10_080);
        assert_eq!(compute_mascot_state(&[agy], 1_100), MascotState::Idle);
    }

    #[test]
    fn idle_vs_sleep_ten_minute_boundary() {
        let mut codex = base_tool(ToolId::Codex);
        codex.last_updated_at = Some(1_000);

        assert_eq!(
            compute_mascot_state(&[codex.clone()], 1_599),
            MascotState::Idle
        );
        assert_eq!(compute_mascot_state(&[codex], 1_600), MascotState::Sleep);
    }

    #[test]
    fn uninstalled_tools_do_not_affect_mascot_state() {
        let mut claude = base_tool(ToolId::Claude);
        claude.installed = false;
        claude.status = AgentStatus::Waiting;
        claude.quotas = vec![QuotaWindow::from_used("five_hour", "5h", 100.0, None, 300)];

        assert_eq!(compute_mascot_state(&[claude], 1_100), MascotState::Sleep);
    }
}
