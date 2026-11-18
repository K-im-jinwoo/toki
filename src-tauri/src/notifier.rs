use std::collections::{HashMap, HashSet};

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::config::TokiConfig;
use crate::schema::{AgentStatus, ToolId, UsageSnapshot};

/// Tracks previously alerted states to avoid spamming duplicate OS notifications.
#[derive(Default)]
pub struct NotifierState {
    prev_status: HashMap<ToolId, AgentStatus>,
    alerted_low_quotas: HashSet<(ToolId, String)>,
}

impl NotifierState {
    pub fn evaluate(&mut self, app: &AppHandle, snapshot: &UsageSnapshot, cfg: &TokiConfig) {
        if !cfg.notifications_enabled {
            return;
        }

        let threshold = cfg.low_quota_threshold_percent;
        let reset_threshold = threshold + 5.0;

        for tool_snap in &snapshot.tools {
            if !tool_snap.installed {
                continue;
            }

            let prev = self.prev_status.insert(tool_snap.tool, tool_snap.status);
            if tool_snap.status == AgentStatus::Waiting && prev != Some(AgentStatus::Waiting) {
                let name = tool_display_name(tool_snap.tool);
                send_notification(
                    app,
                    &format!("{name} 응답 대기 (!)"),
                    "에이전트가 사용자 입력 또는 승인을 기다리고 있습니다.",
                );
            }

            for q in &tool_snap.quotas {
                let key = (tool_snap.tool, q.id.clone());
                if q.remaining_percent <= threshold {
                    if self.alerted_low_quotas.insert(key) {
                        let name = tool_display_name(tool_snap.tool);
                        send_notification(
                            app,
                            &format!("{name} {} 잔여량 부족", q.label),
                            &format!("남은 쿼터가 {:.0}%입니다.", q.remaining_percent),
                        );
                    }
                } else if q.remaining_percent > reset_threshold {
                    self.alerted_low_quotas.remove(&key);
                }
            }
        }
    }
}

fn tool_display_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Codex => "Codex",
        ToolId::Antigravity => "Antigravity",
        ToolId::Claude => "Claude Code",
    }
}

fn send_notification(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}
