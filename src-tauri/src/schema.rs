use serde::{Deserialize, Serialize};

/// Supported AI coding tools monitored by Toki.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    Codex,
    Antigravity,
    Claude,
}

/// Live operational status of a tool or an individual session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Waiting,
    Idle,
    QuotaLow,
    Unavailable,
}

/// Overall mascot state driving tray sprite frames and popover avatar expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MascotState {
    Sleep,
    Idle,
    Run,
    Sprint,
    Alert,
    Exhausted,
}

impl MascotState {
    /// Animation tick interval in milliseconds for each mascot state.
    pub fn frame_interval_ms(self) -> u64 {
        match self {
            MascotState::Sleep => 900,
            MascotState::Idle => 450,
            MascotState::Run => 180,
            MascotState::Sprint => 100,
            MascotState::Alert => 300,
            MascotState::Exhausted => 600,
        }
    }
}

/// Remaining quota window (e.g. "5h" or "Week") in an optional pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub remaining_percent: f64,
    pub used_percent: f64,
    pub resets_at: Option<i64>,
    pub window_minutes: u32,
}

impl QuotaWindow {
    pub fn from_used(
        id: impl Into<String>,
        label: impl Into<String>,
        used_percent: f64,
        resets_at: Option<i64>,
        window_minutes: u32,
    ) -> Self {
        let clamped_used = used_percent.clamp(0.0, 100.0);
        let remaining = (100.0 - clamped_used).clamp(0.0, 100.0);
        Self {
            id: id.into(),
            label: label.into(),
            group: None,
            remaining_percent: remaining,
            used_percent: clamped_used,
            resets_at,
            window_minutes,
        }
    }

    pub fn with_group(mut self, group: Option<String>) -> Self {
        self.group = group;
        self
    }
}

/// Active or recent session summary for display in the popover list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub workspace: Option<String>,
    pub status: AgentStatus,
    pub updated_at: i64,
}

/// Normalized snapshot produced by each tool collector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSnapshot {
    pub tool: ToolId,
    pub installed: bool,
    pub hook_configured: Option<bool>,
    pub status: AgentStatus,
    pub quotas: Vec<QuotaWindow>,
    pub active_sessions: Vec<SessionSummary>,
    pub last_updated_at: Option<i64>,
}

/// Aggregated payload pushed from Rust backend to the React popover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub overall_state: MascotState,
    pub tools: Vec<ToolSnapshot>,
    pub generated_at: i64,
}
