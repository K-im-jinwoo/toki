use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::animator::AnimatorHandle;
use crate::schema::{AgentStatus, QuotaWindow, SessionSummary, ToolId, ToolSnapshot, UsageSnapshot};
use crate::state::build_usage_snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MockScenario {
    Auto,
    Sleep,
    Idle,
    Run,
    Sprint,
    Alert,
    Exhausted,
}

struct MockInner {
    scenario: MockScenario,
    tick: u64,
    latest: UsageSnapshot,
}

#[derive(Clone)]
pub struct MockCollector {
    inner: Arc<Mutex<MockInner>>,
}

impl MockCollector {
    pub fn start_paused() -> Self {
        let now = unix_now();
        let initial = generate_snapshot(MockScenario::Auto, 0, now);
        let inner = Arc::new(Mutex::new(MockInner {
            scenario: MockScenario::Auto,
            tick: 0,
            latest: initial,
        }));
        Self { inner }
    }

    pub fn advance_tick(&self, now: i64) -> UsageSnapshot {
        let Ok(mut guard) = self.inner.lock() else {
            return generate_snapshot(MockScenario::Idle, 0, now);
        };
        guard.tick = guard.tick.wrapping_add(1);
        let snap = generate_snapshot(guard.scenario, guard.tick, now);
        guard.latest = snap.clone();
        snap
    }

    pub fn refresh(&self, now: i64) -> UsageSnapshot {
        let Ok(mut guard) = self.inner.lock() else {
            return generate_snapshot(MockScenario::Idle, 0, now);
        };
        let snap = generate_snapshot(guard.scenario, guard.tick, now);
        guard.latest = snap.clone();
        snap
    }

    pub fn latest(&self) -> UsageSnapshot {
        self.inner
            .lock()
            .map(|g| g.latest.clone())
            .unwrap_or_else(|_| generate_snapshot(MockScenario::Idle, 0, unix_now()))
    }

    pub fn scenario(&self) -> MockScenario {
        self.inner
            .lock()
            .map(|g| g.scenario)
            .unwrap_or(MockScenario::Auto)
    }

    pub fn set_scenario(
        &self,
        scenario: MockScenario,
        app: &AppHandle,
        animator: &AnimatorHandle,
    ) -> UsageSnapshot {
        let now = unix_now();
        let snapshot = {
            let mut guard = self.inner.lock().expect("mock lock poisoned");
            guard.scenario = scenario;
            let snap = generate_snapshot(scenario, guard.tick, now);
            guard.latest = snap.clone();
            snap
        };
        animator.set_state(snapshot.overall_state);
        let _ = app.emit("usage-snapshot", &snapshot);
        snapshot
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn dual_quotas(five_used: f64, week_used: f64, five_reset: i64, week_reset: i64) -> Vec<QuotaWindow> {
    vec![
        QuotaWindow::from_used("five_hour", "5h", five_used, Some(five_reset), 300),
        QuotaWindow::from_used("weekly", "Week", week_used, Some(week_reset), 10_080),
    ]
}

fn agy_quotas(
    five_used: f64,
    week_used: f64,
    argon_used: f64,
    five_reset: i64,
    week_reset: i64,
) -> Vec<QuotaWindow> {
    vec![
        QuotaWindow::from_used("five_hour", "5h", five_used, Some(five_reset), 300),
        QuotaWindow::from_used("weekly", "Week", week_used, Some(week_reset), 10_080),
        QuotaWindow::from_used("argon", "Argon", argon_used, Some(week_reset), 10_080),
    ]
}

fn generate_snapshot(scenario: MockScenario, tick: u64, now: i64) -> UsageSnapshot {
    let effective = match scenario {
        MockScenario::Auto => match (tick / 6) % 5 {
            0 => MockScenario::Run,
            1 => MockScenario::Sprint,
            2 => MockScenario::Alert,
            3 => MockScenario::Idle,
            _ => MockScenario::Exhausted,
        },
        other => other,
    };

    let drift = ((tick % 12) as f64) * 0.4;
    let five_reset = now + 2 * 3600 + 24 * 60 - (tick as i64 % 3600);
    let week_reset = now + 4 * 86400 + 7 * 3600;

    let tools = match effective {
        MockScenario::Sleep => vec![
            ToolSnapshot {
                tool: ToolId::Codex,
                installed: true,
                hook_configured: None,
                status: AgentStatus::Idle,
                quotas: dual_quotas(8.0, 14.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "codex-1".into(),
                    title: "Refactor collector trait".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Idle,
                    updated_at: now - 900,
                }],
                last_updated_at: Some(now - 900),
            },
            ToolSnapshot {
                tool: ToolId::Antigravity,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Idle,
                quotas: agy_quotas(12.0, 19.0, 6.0, five_reset, week_reset),
                active_sessions: vec![],
                last_updated_at: Some(now - 1_200),
            },
            ToolSnapshot {
                tool: ToolId::Claude,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Idle,
                quotas: dual_quotas(5.0, 22.0, five_reset, week_reset),
                active_sessions: vec![],
                last_updated_at: Some(now - 1_800),
            },
        ],
        MockScenario::Idle => vec![
            ToolSnapshot {
                tool: ToolId::Codex,
                installed: true,
                hook_configured: None,
                status: AgentStatus::Idle,
                quotas: dual_quotas(18.0 + drift, 26.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "codex-idle".into(),
                    title: "Add schema unit tests".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Idle,
                    updated_at: now - 45,
                }],
                last_updated_at: Some(now - 45),
            },
            ToolSnapshot {
                tool: ToolId::Antigravity,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Idle,
                quotas: agy_quotas(24.0, 31.0, 15.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "agy-idle".into(),
                    title: "Review tray animation".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Idle,
                    updated_at: now - 120,
                }],
                last_updated_at: Some(now - 120),
            },
            ToolSnapshot {
                tool: ToolId::Claude,
                installed: true,
                hook_configured: Some(false),
                status: AgentStatus::Idle,
                quotas: vec![],
                active_sessions: vec![],
                last_updated_at: Some(now - 300),
            },
        ],
        MockScenario::Run => vec![
            ToolSnapshot {
                tool: ToolId::Codex,
                installed: true,
                hook_configured: None,
                status: AgentStatus::Running,
                quotas: dual_quotas(24.0 + drift, 29.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "codex-run".into(),
                    title: "Implement JSONL parser".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Running,
                    updated_at: now,
                }],
                last_updated_at: Some(now),
            },
            ToolSnapshot {
                tool: ToolId::Antigravity,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Idle,
                quotas: agy_quotas(18.0, 38.0, 12.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "agy-recent".into(),
                    title: "Design Toki UI cards".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Idle,
                    updated_at: now - 90,
                }],
                last_updated_at: Some(now - 90),
            },
            ToolSnapshot {
                tool: ToolId::Claude,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Idle,
                // Single-window example to verify full-width rendering
                quotas: vec![QuotaWindow::from_used(
                    "weekly",
                    "Week",
                    42.0,
                    Some(week_reset),
                    10_080,
                )],
                active_sessions: vec![],
                last_updated_at: Some(now - 240),
            },
        ],
        MockScenario::Sprint => vec![
            ToolSnapshot {
                tool: ToolId::Codex,
                installed: true,
                hook_configured: None,
                status: AgentStatus::Running,
                quotas: dual_quotas(44.0 + drift, 37.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "codex-sprint".into(),
                    title: "Write collector fixtures".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Running,
                    updated_at: now,
                }],
                last_updated_at: Some(now),
            },
            ToolSnapshot {
                tool: ToolId::Antigravity,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Running,
                quotas: agy_quotas(36.0 + drift, 41.0, 22.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "agy-sprint".into(),
                    title: "Build dashboard components".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Running,
                    updated_at: now - 1,
                }],
                last_updated_at: Some(now - 1),
            },
            ToolSnapshot {
                tool: ToolId::Claude,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Running,
                quotas: dual_quotas(28.0 + drift, 49.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "claude-sprint".into(),
                    title: "Run integration tests".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Running,
                    updated_at: now - 2,
                }],
                last_updated_at: Some(now - 2),
            },
        ],
        MockScenario::Alert => vec![
            ToolSnapshot {
                tool: ToolId::Codex,
                installed: true,
                hook_configured: None,
                status: AgentStatus::Waiting,
                quotas: dual_quotas(52.0, 39.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "codex-wait".into(),
                    title: "Approval: run cargo test".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Waiting,
                    updated_at: now - 3,
                }],
                last_updated_at: Some(now - 3),
            },
            ToolSnapshot {
                tool: ToolId::Antigravity,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Running,
                quotas: agy_quotas(30.0, 40.0, 27.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "agy-bg".into(),
                    title: "Build UI shell".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::Running,
                    updated_at: now,
                }],
                last_updated_at: Some(now),
            },
            ToolSnapshot {
                tool: ToolId::Claude,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::Idle,
                quotas: dual_quotas(21.0, 50.0, five_reset, week_reset),
                active_sessions: vec![],
                last_updated_at: Some(now - 180),
            },
        ],
        MockScenario::Exhausted | MockScenario::Auto => vec![
            ToolSnapshot {
                tool: ToolId::Codex,
                installed: true,
                hook_configured: None,
                status: AgentStatus::QuotaLow,
                quotas: dual_quotas(97.0, 84.0, five_reset, week_reset),
                active_sessions: vec![SessionSummary {
                    id: "codex-low".into(),
                    title: "Rate limit near cap".into(),
                    workspace: Some("toki".into()),
                    status: AgentStatus::QuotaLow,
                    updated_at: now - 30,
                }],
                last_updated_at: Some(now - 30),
            },
            ToolSnapshot {
                tool: ToolId::Antigravity,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::QuotaLow,
                quotas: agy_quotas(45.0, 52.0, 96.0, five_reset, week_reset),
                active_sessions: vec![],
                last_updated_at: Some(now - 240),
            },
            ToolSnapshot {
                tool: ToolId::Claude,
                installed: true,
                hook_configured: Some(true),
                status: AgentStatus::QuotaLow,
                quotas: dual_quotas(96.0, 91.0, five_reset, week_reset),
                active_sessions: vec![],
                last_updated_at: Some(now - 120),
            },
        ],
    };

    build_usage_snapshot(tools, now)
}
