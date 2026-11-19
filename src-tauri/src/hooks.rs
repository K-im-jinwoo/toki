use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::collectors::detect_hook_configured;
use crate::schema::ToolId;

const SH_HOOK_SCRIPT: &str = include_str!("../../resources/hooks/toki-statusline-hook.sh");
const PS1_HOOK_SCRIPT: &str = include_str!("../../resources/hooks/toki-statusline-hook.ps1");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookStatus {
    pub antigravity_configured: bool,
    pub claude_configured: bool,
    pub hook_script_path: String,
}

pub fn ensure_hook_scripts_installed(home: &Path) -> Result<PathBuf, String> {
    let bin_dir = home.join(".toki").join("bin");
    fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    let sh_path = bin_dir.join("toki-statusline-hook.sh");
    fs::write(&sh_path, SH_HOOK_SCRIPT).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&sh_path, perms).map_err(|e| e.to_string())?;
    }

    let ps1_path = bin_dir.join("toki-statusline-hook.ps1");
    fs::write(&ps1_path, PS1_HOOK_SCRIPT).map_err(|e| e.to_string())?;

    if cfg!(target_os = "windows") {
        Ok(ps1_path)
    } else {
        Ok(sh_path)
    }
}

pub fn get_hook_status(
    home: &Path,
    antigravity_settings: &Path,
    claude_settings: &Path,
) -> HookStatus {
    let script_path = if cfg!(target_os = "windows") {
        home.join(".toki")
            .join("bin")
            .join("toki-statusline-hook.ps1")
    } else {
        home.join(".toki")
            .join("bin")
            .join("toki-statusline-hook.sh")
    };

    HookStatus {
        antigravity_configured: detect_hook_configured(antigravity_settings),
        claude_configured: detect_hook_configured(claude_settings),
        hook_script_path: script_path.to_string_lossy().to_string(),
    }
}

pub fn install_hook_for_tool(
    home: &Path,
    tool: ToolId,
    settings_path: &Path,
) -> Result<(), String> {
    install_hook_for_tool_os(home, tool, settings_path, cfg!(target_os = "windows"))
}

fn install_hook_for_tool_os(
    home: &Path,
    tool: ToolId,
    settings_path: &Path,
    is_windows: bool,
) -> Result<(), String> {
    let script_path = if is_windows {
        ensure_hook_scripts_installed(home)?;
        home.join(".toki")
            .join("bin")
            .join("toki-statusline-hook.ps1")
    } else {
        ensure_hook_scripts_installed(home)?
    };
    let tool_key = match tool {
        ToolId::Antigravity => "antigravity",
        ToolId::Claude => "claude",
        ToolId::Codex => return Err("Codex does not require a statusLine hook".into()),
    };

    let mut root = read_json_object(settings_path)?;
    let existing_cmd = root
        .get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .map(String::from);

    let base_cmd = build_hook_command_for_os(&script_path, tool_key, is_windows);
    let final_cmd = match existing_cmd {
        Some(prev) if !is_toki_command(&prev) && !prev.trim().is_empty() => {
            format!("{base_cmd} {prev}")
        }
        Some(prev) if is_toki_command(&prev) => {
            if let Some(orig) = extract_chained_original(&prev, tool_key) {
                format!("{base_cmd} {orig}")
            } else {
                base_cmd
            }
        }
        _ => base_cmd,
    };

    let mut sl_obj = root
        .get("statusLine")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    sl_obj.insert("type".into(), json!("command"));
    sl_obj.insert("command".into(), json!(final_cmd));
    if tool == ToolId::Antigravity {
        sl_obj.insert("enabled".into(), json!(true));
        sl_obj.insert("stack_with_default".into(), json!(true));
    }

    root.insert("statusLine".into(), Value::Object(sl_obj));
    write_json_object_atomic(settings_path, &root)
}

pub fn repair_installed_hooks(
    home: &Path,
    antigravity_settings: &Path,
    claude_settings: &Path,
) {
    let agy_configured = detect_hook_configured(antigravity_settings);
    let claude_configured = detect_hook_configured(claude_settings);
    if !agy_configured && !claude_configured {
        return;
    }

    let _ = ensure_hook_scripts_installed(home);

    if agy_configured {
        let _ = repair_installed_hook_for_os(
            home,
            ToolId::Antigravity,
            antigravity_settings,
            cfg!(target_os = "windows"),
        );
    }
    if claude_configured {
        let _ = repair_installed_hook_for_os(
            home,
            ToolId::Claude,
            claude_settings,
            cfg!(target_os = "windows"),
        );
    }
}

fn repair_installed_hook_for_os(
    home: &Path,
    tool: ToolId,
    settings_path: &Path,
    is_windows: bool,
) -> Result<(), String> {
    if !is_windows || !settings_path.exists() {
        return Ok(());
    }
    let root = read_json_object(settings_path)?;
    let Some(cmd) = root
        .get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
    else {
        return Ok(());
    };

    if needs_windows_hook_repair(cmd) {
        install_hook_for_tool_os(home, tool, settings_path, true)?;
    }
    Ok(())
}

fn needs_windows_hook_repair(cmd: &str) -> bool {
    is_toki_command(cmd) && cmd.contains("toki-statusline-hook.ps1\"")
}

pub fn uninstall_hook_for_tool(tool: ToolId, settings_path: &Path) -> Result<(), String> {
    if !settings_path.exists() {
        return Ok(());
    }
    let tool_key = match tool {
        ToolId::Antigravity => "antigravity",
        ToolId::Claude => "claude",
        ToolId::Codex => return Ok(()),
    };

    let mut root = read_json_object(settings_path)?;
    let existing_cmd = root
        .get("statusLine")
        .and_then(|sl| sl.get("command"))
        .and_then(|c| c.as_str())
        .map(String::from);

    let Some(cmd) = existing_cmd else {
        return Ok(());
    };
    if !is_toki_command(&cmd) {
        return Ok(());
    }

    if let Some(original_cmd) = extract_chained_original(&cmd, tool_key) {
        if let Some(Value::Object(sl_obj)) = root.get_mut("statusLine") {
            sl_obj.insert("command".into(), json!(original_cmd));
        }
    } else {
        root.remove("statusLine");
    }

    write_json_object_atomic(settings_path, &root)
}

fn build_hook_command_for_os(script_path: &Path, tool_key: &str, is_windows: bool) -> String {
    let escaped = script_path.to_string_lossy();
    if is_windows {
        // Avoid double quotes on Windows: Go-based runners (e.g. agy) pass `"..."` through
        // cmd/EscapeArg or split on whitespace, leaving literal `"` chars in `-File`.
        if escaped.chars().any(char::is_whitespace) {
            let ps_single_quoted = escaped.replace('\'', "''");
            format!(
                "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command . '{ps_single_quoted}' {tool_key}"
            )
        } else {
            format!(
                "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File {escaped} {tool_key}"
            )
        }
    } else {
        format!("sh \"{escaped}\" {tool_key}")
    }
}

fn is_toki_command(cmd: &str) -> bool {
    cmd.contains("toki-statusline-hook") || cmd.contains("toki")
}

fn extract_chained_original(cmd: &str, tool_key: &str) -> Option<String> {
    let search_start = cmd.find("toki-statusline-hook").unwrap_or(0);
    let marker = format!(" {tool_key} ");
    let rel_idx = cmd[search_start..].find(&marker)?;
    let rest = cmd[search_start + rel_idx + marker.len()..].trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(Map::new());
    }
    let val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    val.as_object()
        .cloned()
        .ok_or_else(|| "settings.json root is not a JSON object".to_string())
}

fn write_json_object_atomic(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.toki.tmp");
    let pretty = serde_json::to_string_pretty(&Value::Object(map.clone())).map_err(|e| e.to_string())?;
    fs::write(&tmp, format!("{pretty}\n")).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preserves_existing_settings_chains_command_and_restores_on_uninstall() {
        let tmp = std::env::temp_dir().join(format!(
            "toki-hook-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_settings = tmp.join(".claude").join("settings.json");
        fs::create_dir_all(claude_settings.parent().unwrap()).unwrap();
        fs::write(
            &claude_settings,
            r#"{
  "theme": "dark",
  "statusLine": {
    "type": "command",
    "command": "bash ~/.claude/custom-status.sh"
  }
}"#,
        )
        .unwrap();

        install_hook_for_tool(&tmp, ToolId::Claude, &claude_settings).unwrap();
        let after_install = read_json_object(&claude_settings).unwrap();
        assert_eq!(after_install.get("theme").unwrap(), "dark");
        let cmd = after_install["statusLine"]["command"].as_str().unwrap();
        assert!(cmd.contains("toki-statusline-hook"));
        assert!(cmd.ends_with("bash ~/.claude/custom-status.sh"));

        uninstall_hook_for_tool(ToolId::Claude, &claude_settings).unwrap();
        let after_uninstall = read_json_object(&claude_settings).unwrap();
        assert_eq!(after_uninstall.get("theme").unwrap(), "dark");
        assert_eq!(
            after_uninstall["statusLine"]["command"].as_str().unwrap(),
            "bash ~/.claude/custom-status.sh"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn antigravity_install_sets_stack_with_default_and_removes_cleanly() {
        let tmp = std::env::temp_dir().join(format!(
            "toki-agy-hook-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let agy_settings = tmp.join(".gemini").join("antigravity-cli").join("settings.json");
        fs::create_dir_all(agy_settings.parent().unwrap()).unwrap();
        fs::write(&agy_settings, r#"{"colorScheme":"tokyo night"}"#).unwrap();

        install_hook_for_tool(&tmp, ToolId::Antigravity, &agy_settings).unwrap();
        let after = read_json_object(&agy_settings).unwrap();
        assert_eq!(after["colorScheme"], "tokyo night");
        assert_eq!(after["statusLine"]["stack_with_default"], true);
        assert!(after["statusLine"]["command"]
            .as_str()
            .unwrap()
            .contains("antigravity"));

        uninstall_hook_for_tool(ToolId::Antigravity, &agy_settings).unwrap();
        let removed = read_json_object(&agy_settings).unwrap();
        assert_eq!(removed["colorScheme"], "tokyo night");
        assert!(removed.get("statusLine").is_none());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn windows_hook_command_never_uses_double_quotes_and_handles_spaces() {
        let plain = Path::new(r"C:\Users\USER\.toki\bin\toki-statusline-hook.ps1");
        let cmd_plain = build_hook_command_for_os(plain, "antigravity", true);
        assert!(!cmd_plain.contains('"'));
        assert_eq!(
            cmd_plain,
            r"powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File C:\Users\USER\.toki\bin\toki-statusline-hook.ps1 antigravity"
        );

        let spaced = Path::new(r"C:\Users\O'Brien Jin\.toki\bin\toki-statusline-hook.ps1");
        let cmd_spaced = build_hook_command_for_os(spaced, "antigravity", true);
        assert!(!cmd_spaced.contains('"'));
        assert_eq!(
            cmd_spaced,
            r"powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command . 'C:\Users\O''Brien Jin\.toki\bin\toki-statusline-hook.ps1' antigravity"
        );
    }

    #[test]
    fn repairs_legacy_quoted_windows_hook_preserving_chained_command() {
        let tmp = std::env::temp_dir().join(format!(
            "toki-win-repair-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let agy_settings = tmp.join(".gemini").join("antigravity-cli").join("settings.json");
        fs::create_dir_all(agy_settings.parent().unwrap()).unwrap();
        fs::write(
            &agy_settings,
            r#"{
  "colorScheme": "tokyo night",
  "statusLine": {
    "type": "command",
    "command": "powershell -NoProfile -ExecutionPolicy Bypass -File \"C:\\Users\\USER\\.toki\\bin\\toki-statusline-hook.ps1\" antigravity node custom.js",
    "enabled": true,
    "stack_with_default": true
  }
}"#,
        )
        .unwrap();

        repair_installed_hook_for_os(&tmp, ToolId::Antigravity, &agy_settings, true).unwrap();
        let repaired = read_json_object(&agy_settings).unwrap();
        assert_eq!(repaired["colorScheme"], "tokyo night");
        let cmd = repaired["statusLine"]["command"].as_str().unwrap();
        assert!(!cmd.contains('"'));
        assert!(cmd.contains(" -File "));
        assert!(cmd.ends_with(" antigravity node custom.js"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
