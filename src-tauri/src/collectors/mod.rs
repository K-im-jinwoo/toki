pub mod antigravity;
pub mod claude;
pub mod codex;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDateTime};
use notify::{Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::animator::AnimatorHandle;
use crate::config::{load_config, save_config, TokiConfig};
use crate::mock::{MockCollector, MockScenario};
use crate::notifier::NotifierState;
use crate::schema::{AgentStatus, QuotaWindow, SessionSummary, UsageSnapshot};
use crate::state::{build_usage_snapshot, EXHAUSTED_THRESHOLD_PERCENT};

use self::antigravity::{collect_antigravity, AntigravityConfig};
use self::claude::{collect_claude, ClaudeConfig};
use self::codex::{collect_codex, CodexConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataMode {
    Live,
    Mock,
}

#[derive(Clone)]
pub struct CollectorPaths {
    pub home: PathBuf,
    pub codex: CodexConfig,
    pub antigravity: AntigravityConfig,
    pub claude: ClaudeConfig,
}

impl CollectorPaths {
    pub fn from_config(home: &Path, cfg: &TokiConfig) -> Self {
        let mut codex = CodexConfig::default_from_home(home);
        if let Some(custom) = cfg
            .custom_paths
            .codex_sessions_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            codex.sessions_dir = expand_tilde(home, custom);
        }

        let mut antigravity = AntigravityConfig::default_from_home(home);
        if let Some(custom) = cfg
            .custom_paths
            .antigravity_cli_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            antigravity.cli_dir = expand_tilde(home, custom);
        }

        let mut claude = ClaudeConfig::default_from_home(home);
        if let Some(custom) = cfg
            .custom_paths
            .claude_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            claude.claude_dir = expand_tilde(home, custom);
        }

        Self {
            home: home.to_path_buf(),
            codex,
            antigravity,
            claude,
        }
    }
}

fn expand_tilde(home: &Path, raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else if raw == "~" {
        home.to_path_buf()
    } else {
        PathBuf::from(raw)
    }
}

struct ManagerInner {
    mode: DataMode,
    config: TokiConfig,
    paths: CollectorPaths,
    latest_live: UsageSnapshot,
    notifier: NotifierState,
}

#[derive(Clone)]
pub struct CollectorManager {
    inner: Arc<Mutex<ManagerInner>>,
    mock: MockCollector,
}

impl CollectorManager {
    pub fn start(app: AppHandle, animator: AnimatorHandle) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config = load_config(&home);
        let paths = CollectorPaths::from_config(&home, &config);
        crate::hooks::repair_installed_hooks(
            &paths.home,
            &paths.antigravity.cli_dir.join("settings.json"),
            &paths.claude.claude_dir.join("settings.json"),
        );
        let initial_live = collect_all_live(&paths, &config, unix_now());
        animator.set_state(initial_live.overall_state);

        let mock = MockCollector::start_paused();
        let inner = Arc::new(Mutex::new(ManagerInner {
            mode: DataMode::Live,
            config,
            paths: paths.clone(),
            latest_live: initial_live,
            notifier: NotifierState::default(),
        }));

        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let _watcher_handle = std::thread::Builder::new()
            .name("toki-fs-watcher".into())
            .spawn({
                let paths = paths.clone();
                move || {
                    let tx_cb = tx.clone();
                    let mut watcher = match notify::recommended_watcher(
                        move |res: notify::Result<Event>| {
                            if res.is_ok() {
                                let _ = tx_cb.send(());
                            }
                        },
                    ) {
                        Ok(w) => w,
                        Err(_) => return,
                    };

                    for watch_dir in [
                        paths.codex.sessions_dir.clone(),
                        paths.antigravity.cli_dir.clone(),
                        paths.antigravity.desktop_dir.clone(),
                        paths.claude.claude_dir.join("projects"),
                        paths
                            .antigravity
                            .hook_cache_path
                            .parent()
                            .unwrap_or_else(|| Path::new("."))
                            .to_path_buf(),
                    ] {
                        if watch_dir.exists() {
                            let _ = watcher.watch(&watch_dir, RecursiveMode::Recursive);
                        }
                    }

                    loop {
                        std::thread::sleep(Duration::from_secs(60));
                    }
                }
            });

        let worker_inner = Arc::clone(&inner);
        let worker_mock = mock.clone();
        std::thread::Builder::new()
            .name("toki-collector-loop".into())
            .spawn(move || loop {
                if rx.recv_timeout(Duration::from_millis(1500)).is_ok() {
                    std::thread::sleep(Duration::from_millis(150));
                    while rx.try_recv().is_ok() {}
                }

                let now = unix_now();
                let (mode, cfg, paths_snapshot) = {
                    let Ok(guard) = worker_inner.lock() else {
                        break;
                    };
                    (guard.mode, guard.config.clone(), guard.paths.clone())
                };

                let snapshot = match mode {
                    DataMode::Live => {
                        let snap = collect_all_live(&paths_snapshot, &cfg, now);
                        if let Ok(mut guard) = worker_inner.lock() {
                            guard.latest_live = snap.clone();
                            guard.notifier.evaluate(&app, &snap, &cfg);
                        }
                        snap
                    }
                    DataMode::Mock => worker_mock.advance_tick(now),
                };

                animator.set_state(snapshot.overall_state);
                let _ = app.emit("usage-snapshot", &snapshot);
            })
            .expect("failed to spawn collector loop");

        Self { inner, mock }
    }

    pub fn current_snapshot(&self) -> UsageSnapshot {
        let (mode, cfg, paths, cached) = {
            let guard = self.inner.lock().expect("collector lock poisoned");
            (
                guard.mode,
                guard.config.clone(),
                guard.paths.clone(),
                guard.latest_live.clone(),
            )
        };
        match mode {
            DataMode::Live => {
                if cached.tools.is_empty() && cfg.visible_tools.codex {
                    collect_all_live(&paths, &cfg, unix_now())
                } else {
                    cached
                }
            }
            DataMode::Mock => self.mock.latest(),
        }
    }

    pub fn config(&self) -> TokiConfig {
        self.inner
            .lock()
            .map(|g| g.config.clone())
            .unwrap_or_default()
    }

    pub fn paths(&self) -> CollectorPaths {
        self.inner
            .lock()
            .map(|g| g.paths.clone())
            .unwrap_or_else(|_| {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                CollectorPaths::from_config(&home, &TokiConfig::default())
            })
    }

    pub fn update_config(
        &self,
        new_cfg: TokiConfig,
        app: &AppHandle,
        animator: &AnimatorHandle,
    ) -> Result<UsageSnapshot, String> {
        let now = unix_now();
        let (paths, mode) = {
            let mut guard = self.inner.lock().map_err(|e| e.to_string())?;
            save_config(&guard.paths.home, &new_cfg)?;
            let new_paths = CollectorPaths::from_config(&guard.paths.home, &new_cfg);
            guard.config = new_cfg.clone();
            guard.paths = new_paths.clone();
            (new_paths, guard.mode)
        };

        let snapshot = if mode == DataMode::Live {
            let snap = collect_all_live(&paths, &new_cfg, now);
            if let Ok(mut guard) = self.inner.lock() {
                guard.latest_live = snap.clone();
            }
            snap
        } else {
            self.mock.refresh(now)
        };

        animator.set_state(snapshot.overall_state);
        let _ = app.emit("usage-snapshot", &snapshot);
        Ok(snapshot)
    }

    pub fn refresh_live_now(&self, app: &AppHandle, animator: &AnimatorHandle) -> UsageSnapshot {
        let now = unix_now();
        let (cfg, paths, mode) = {
            let guard = self.inner.lock().expect("collector lock poisoned");
            (guard.config.clone(), guard.paths.clone(), guard.mode)
        };
        let snapshot = if mode == DataMode::Live {
            let snap = collect_all_live(&paths, &cfg, now);
            if let Ok(mut guard) = self.inner.lock() {
                guard.latest_live = snap.clone();
            }
            snap
        } else {
            self.mock.refresh(now)
        };
        animator.set_state(snapshot.overall_state);
        let _ = app.emit("usage-snapshot", &snapshot);
        snapshot
    }

    pub fn mode(&self) -> DataMode {
        self.inner.lock().map(|g| g.mode).unwrap_or(DataMode::Live)
    }

    pub fn set_mode(
        &self,
        mode: DataMode,
        app: &AppHandle,
        animator: &AnimatorHandle,
    ) -> UsageSnapshot {
        let now = unix_now();
        let (cfg, paths) = {
            let mut guard = self.inner.lock().expect("collector lock poisoned");
            guard.mode = mode;
            (guard.config.clone(), guard.paths.clone())
        };

        let snapshot = match mode {
            DataMode::Live => {
                let snap = collect_all_live(&paths, &cfg, now);
                if let Ok(mut guard) = self.inner.lock() {
                    guard.latest_live = snap.clone();
                }
                snap
            }
            DataMode::Mock => self.mock.refresh(now),
        };
        animator.set_state(snapshot.overall_state);
        let _ = app.emit("usage-snapshot", &snapshot);
        snapshot
    }

    pub fn mock_scenario(&self) -> MockScenario {
        self.mock.scenario()
    }

    pub fn set_mock_scenario(
        &self,
        scenario: MockScenario,
        app: &AppHandle,
        animator: &AnimatorHandle,
    ) -> UsageSnapshot {
        if let Ok(mut guard) = self.inner.lock() {
            guard.mode = DataMode::Mock;
        }
        self.mock.set_scenario(scenario, app, animator)
    }
}

pub fn collect_all_live(
    paths: &CollectorPaths,
    cfg: &TokiConfig,
    now_unix: i64,
) -> UsageSnapshot {
    let mut tools = Vec::new();
    if cfg.visible_tools.codex {
        tools.push(collect_codex(&paths.codex, now_unix));
    }
    if cfg.visible_tools.antigravity {
        tools.push(collect_antigravity(&paths.antigravity, now_unix));
    }
    if cfg.visible_tools.claude {
        tools.push(collect_claude(&paths.claude, now_unix));
    }
    build_usage_snapshot(tools, now_unix)
}

pub fn derive_tool_status(
    installed: bool,
    sessions: &[SessionSummary],
    quotas: &[QuotaWindow],
) -> AgentStatus {
    if !installed {
        return AgentStatus::Unavailable;
    }
    if sessions.iter().any(|s| s.status == AgentStatus::Waiting) {
        return AgentStatus::Waiting;
    }
    if sessions.iter().any(|s| s.status == AgentStatus::Running) {
        return AgentStatus::Running;
    }
    if quotas
        .iter()
        .any(|q| q.remaining_percent <= EXHAUSTED_THRESHOLD_PERCENT)
    {
        return AgentStatus::QuotaLow;
    }
    AgentStatus::Idle
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn file_mtime_unix(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

pub fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_files_recursive(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

pub fn basename_from_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return None;
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .filter(|s| !s.is_empty())
}

pub fn parse_timestamp_to_unix(raw: &str) -> Option<i64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Some(dt.timestamp());
        }
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(ndt.and_utc().timestamp());
        }
    }
    None
}

pub fn detect_hook_configured(settings_path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(settings_path) else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    val.get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .map(|cmd| cmd.contains("toki") || cmd.contains("statusline-hook"))
        .unwrap_or(false)
}

/// Shared defensive parser for `~/.toki/hooks/<tool>.json`.
pub fn parse_hook_file(path: &Path, now_unix: i64) -> (Option<i64>, Vec<QuotaWindow>) {
    let Some((updated_at, payload)) = read_hook_payload(path) else {
        return (None, vec![]);
    };
    (
        updated_at,
        parse_hook_quotas(&payload, updated_at, now_unix),
    )
}

fn read_hook_payload(path: &Path) -> Option<(Option<i64>, Value)> {
    let file = fs::File::open(path).ok()?;
    let mut value: Value = serde_json::from_reader(std::io::BufReader::new(&file)).ok()?;
    let updated_at = value.get("updated_at").and_then(Value::as_i64).or_else(|| {
        // The hook replaces the cache atomically. Use the timestamp of the file we read.
        let modified = file.metadata().ok()?.modified().ok()?;
        Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
    });
    let payload = match value.get_mut("raw") {
        Some(raw) if raw.is_object() => raw.take(),
        _ => value,
    };
    Some((updated_at, payload))
}

fn parse_hook_quotas(payload: &Value, updated_at: Option<i64>, now_unix: i64) -> Vec<QuotaWindow> {
    let mut buckets = std::collections::HashMap::new();
    for container in ["quota", "rate_limits", "quotas"]
        .iter()
        .filter_map(|key| payload.get(key).and_then(Value::as_object))
    {
        for (key, bucket) in container {
            let id = key.trim().to_lowercase();
            if id.is_empty() || buckets.contains_key(&id) {
                continue;
            }
            if let Some(parsed) = parse_hook_bucket(id.clone(), bucket, updated_at, now_unix) {
                buckets.insert(id, parsed);
            }
        }
    }

    let mut windows: Vec<_> = buckets.into_values().collect();
    windows.sort_by_cached_key(|(group_order, window_order, quota)| {
        (
            *group_order,
            quota.group.as_deref().unwrap_or_default().to_lowercase(),
            *window_order,
            quota.window_minutes,
            quota.id.clone(),
        )
    });
    windows.into_iter().map(|(_, _, quota)| quota).collect()
}

fn parse_hook_bucket(
    id: String,
    bucket: &Value,
    hook_updated_at: Option<i64>,
    now_unix: i64,
) -> Option<(u8, u8, QuotaWindow)> {
    let obj = bucket.as_object()?;
    let used_opt = obj
        .get("remaining_fraction")
        .and_then(|v| v.as_f64())
        .map(|frac| (1.0 - frac.clamp(0.0, 1.0)) * 100.0)
        .or_else(|| {
            obj.get("used_percentage")
                .and_then(Value::as_f64)
                .or_else(|| obj.get("used_percent").and_then(Value::as_f64))
        })
        .or_else(|| {
            obj.get("remaining_percentage")
                .and_then(Value::as_f64)
                .or_else(|| obj.get("remaining_percent").and_then(Value::as_f64))
                .map(|rem| 100.0 - rem)
        });

    let used = used_opt?;

    let resets_at = obj
        .get("resets_at")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            obj.get("reset_time")
                .and_then(|v| v.as_str())
                .and_then(parse_timestamp_to_unix)
        })
        .or_else(|| {
            obj.get("reset_in_seconds")
                .and_then(|v| v.as_i64())
                .and_then(|secs| hook_updated_at.unwrap_or(now_unix).checked_add(secs))
        });

    let effective_used = match resets_at {
        Some(r) if r <= now_unix => 0.0,
        _ => used,
    };

    let (group, group_order, label, minutes, window_order) = normalize_bucket_key(&id);
    let quota =
        QuotaWindow::from_used(id, label, effective_used, resets_at, minutes).with_group(group);
    Some((group_order, window_order, quota))
}

fn normalize_bucket_key(id: &str) -> (Option<String>, u8, String, u32, u8) {
    let mut tokens = id
        .split(['-', '_'])
        .filter(|token| !token.is_empty())
        .peekable();
    let mut window = None;
    let mut group_tokens = Vec::new();

    while let Some(token) = tokens.next() {
        let known = match (token, tokens.peek().copied()) {
            ("five", Some("hour")) => Some(("5h", 300, 0, true)),
            ("one", Some("day")) => Some(("Day", 1_440, 1, true)),
            ("seven", Some("day")) => Some(("Week", 10_080, 2, true)),
            ("5h" | "5hr", _) => Some(("5h", 300, 0, false)),
            ("daily" | "1d" | "24h", _) => Some(("Day", 1_440, 1, false)),
            ("weekly" | "7d" | "week", _) => Some(("Week", 10_080, 2, false)),
            ("monthly" | "30d" | "month", _) => Some(("Month", 43_200, 3, false)),
            _ => None,
        };
        let candidate = if let Some((label, minutes, order, paired)) = known {
            if paired {
                tokens.next();
            }
            Some((label, minutes, order))
        } else {
            parse_duration_token(token)
        };

        if let Some(candidate) = candidate {
            window.get_or_insert(candidate);
        } else if token != "wtu" {
            group_tokens.push(token);
        }
    }

    let (group, group_order) = match group_tokens.as_slice() {
        [] => (None, 0),
        ["gemini"] => (Some("Gemini".into()), 0),
        ["3p"] => (Some("3P".into()), 1),
        ["argon"] | ["gemini", "argon"] => (Some("Argon".into()), 2),
        _ => {
            let name = group_tokens
                .into_iter()
                .map(|token| match token {
                    "3p" | "ai" | "api" | "gpt" => token.to_uppercase(),
                    _ => {
                        let mut chars = token.chars();
                        let first = chars.next().expect("empty tokens are filtered out");
                        format!("{}{}", first.to_uppercase(), chars.as_str())
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            (Some(name), 10)
        }
    };
    let (label, minutes, window_order) = window.unwrap_or(("Quota", 0, 99));
    (group, group_order, label.into(), minutes, window_order)
}

fn parse_duration_token(token: &str) -> Option<(&str, u32, u8)> {
    let (count, multiplier, order) = if let Some(hours) = token.strip_suffix('h') {
        (hours, 60, 1)
    } else {
        (token.strip_suffix('d')?, 1_440, 2)
    };
    if !count.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let count = count.parse::<u32>().ok()?;
    let minutes = count
        .checked_mul(multiplier)
        .filter(|minutes| *minutes > 0)?;
    Some((token, minutes, order))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_gemini_and_3p_buckets_without_argon() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/antigravity_hook_multi_pool.json");
        let (updated_at, quotas) = parse_hook_file(&path, 1_800_000_010);

        assert_eq!(updated_at, Some(1_800_000_000));
        assert_eq!(
            quotas
                .iter()
                .map(|q| (q.id.as_str(), q.label.as_str()))
                .collect::<Vec<_>>(),
            [
                ("gemini-5h", "5h"),
                ("gemini-weekly", "Week"),
                ("3p-5h", "5h"),
                ("3p-weekly", "Week")
            ]
        );
        assert_eq!(
            quotas
                .iter()
                .map(|q| q.group.as_deref())
                .collect::<Vec<_>>(),
            [Some("Gemini"), Some("Gemini"), Some("3P"), Some("3P")]
        );
        assert_eq!(quotas[2].remaining_percent, 5.0);
        assert_eq!(
            derive_tool_status(true, &[], &quotas),
            AgentStatus::QuotaLow
        );
        assert_eq!(quotas[1].resets_at, Some(1_800_600_000));

        let (_, expired) = parse_hook_file(&path, 1_800_000_100);
        assert_eq!(expired[2].remaining_percent, 100.0);
    }

    #[test]
    fn discovers_future_buckets_across_quota_objects() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/future_hook_quotas.json");
        let (_, quotas) = parse_hook_file(&path, 1_800_000_000);

        assert_eq!(
            quotas
                .iter()
                .map(|q| (q.id.as_str(), q.label.as_str(), q.window_minutes))
                .collect::<Vec<_>>(),
            [
                ("ai", "Quota", 0),
                ("api-12h", "12h", 720),
                ("api-2d", "2d", 2880),
                ("flash-daily", "Day", 1440),
                ("gpt-month", "Month", 43200),
                ("opus-5h", "5h", 300),
                ("opus-weekly", "Week", 10080)
            ]
        );
        assert_eq!(
            quotas
                .iter()
                .map(|q| q.group.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("AI"),
                Some("API"),
                Some("API"),
                Some("Flash"),
                Some("GPT"),
                Some("Opus"),
                Some("Opus")
            ]
        );
        assert_eq!(quotas[6].remaining_percent, 61.0);
    }
}
