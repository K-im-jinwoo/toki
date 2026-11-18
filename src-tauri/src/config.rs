use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::schema::ToolId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibleToolsConfig {
    #[serde(default = "default_true")]
    pub codex: bool,
    #[serde(default = "default_true")]
    pub antigravity: bool,
    #[serde(default = "default_true")]
    pub claude: bool,
}

impl Default for VisibleToolsConfig {
    fn default() -> Self {
        Self {
            codex: true,
            antigravity: true,
            claude: true,
        }
    }
}

impl VisibleToolsConfig {
    pub fn is_visible(&self, tool: ToolId) -> bool {
        match tool {
            ToolId::Codex => self.codex,
            ToolId::Antigravity => self.antigravity,
            ToolId::Claude => self.claude,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPathsConfig {
    pub codex_sessions_dir: Option<String>,
    pub antigravity_cli_dir: Option<String>,
    pub claude_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokiConfig {
    #[serde(default)]
    pub visible_tools: VisibleToolsConfig,
    #[serde(default)]
    pub custom_paths: CustomPathsConfig,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default = "default_threshold")]
    pub low_quota_threshold_percent: f64,
}

impl Default for TokiConfig {
    fn default() -> Self {
        Self {
            visible_tools: VisibleToolsConfig::default(),
            custom_paths: CustomPathsConfig::default(),
            notifications_enabled: true,
            low_quota_threshold_percent: 10.0,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_threshold() -> f64 {
    10.0
}

pub fn config_file_path(home: &Path) -> PathBuf {
    home.join(".toki").join("config.json")
}

pub fn load_config(home: &Path) -> TokiConfig {
    let path = config_file_path(home);
    let Ok(content) = fs::read_to_string(&path) else {
        return TokiConfig::default();
    };
    serde_json::from_str::<TokiConfig>(&content).unwrap_or_default()
}

pub fn save_config(home: &Path, cfg: &TokiConfig) -> Result<(), String> {
    let path = config_file_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn round_trips_config_and_defaults_safely() {
        let tmp_home = std::env::temp_dir().join(format!(
            "toki-cfg-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut cfg = load_config(&tmp_home);
        assert!(cfg.visible_tools.codex);
        assert!(cfg.visible_tools.antigravity);
        assert!(cfg.visible_tools.claude);

        cfg.visible_tools.claude = false;
        cfg.low_quota_threshold_percent = 20.0;
        cfg.custom_paths.codex_sessions_dir = Some("/custom/codex".into());
        save_config(&tmp_home, &cfg).unwrap();

        let reloaded = load_config(&tmp_home);
        assert_eq!(reloaded, cfg);

        let _ = fs::remove_dir_all(&tmp_home);
    }
}
