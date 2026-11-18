pub mod animator;
pub mod collectors;
pub mod config;
pub mod hooks;
pub mod mock;
pub mod notifier;
pub mod schema;
pub mod state;

use std::sync::Mutex;

use animator::{AnimatorHandle, SpriteBank};
use collectors::{CollectorManager, DataMode};
use config::TokiConfig;
use hooks::{
    get_hook_status as read_hook_status, install_hook_for_tool, uninstall_hook_for_tool,
    HookStatus,
};
use mock::MockScenario;
use schema::{MascotState, ToolId, UsageSnapshot};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::{Effect, EffectState, EffectsBuilder},
    AppHandle, LogicalSize, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
    WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

const POPOVER_WIDTH: f64 = 368.0;

struct AppState {
    animator: AnimatorHandle,
    collectors: CollectorManager,
    last_tray_rect: Mutex<Option<Rect>>,
}

#[tauri::command]
fn get_usage_snapshot(state: tauri::State<'_, AppState>) -> UsageSnapshot {
    state.collectors.current_snapshot()
}

#[tauri::command]
fn get_data_mode(state: tauri::State<'_, AppState>) -> DataMode {
    state.collectors.mode()
}

#[tauri::command]
fn set_data_mode(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    mode: DataMode,
) -> UsageSnapshot {
    state.collectors.set_mode(mode, &app, &state.animator)
}

#[tauri::command]
fn get_mock_scenario(state: tauri::State<'_, AppState>) -> MockScenario {
    state.collectors.mock_scenario()
}

#[tauri::command]
fn set_mock_scenario(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    scenario: MockScenario,
) -> UsageSnapshot {
    state
        .collectors
        .set_mock_scenario(scenario, &app, &state.animator)
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> TokiConfig {
    state.collectors.config()
}

#[tauri::command]
fn save_config(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: TokiConfig,
) -> Result<UsageSnapshot, String> {
    state
        .collectors
        .update_config(config, &app, &state.animator)
}

#[tauri::command]
fn get_hook_status(state: tauri::State<'_, AppState>) -> HookStatus {
    let paths = state.collectors.paths();
    read_hook_status(
        &paths.home,
        &paths.antigravity.cli_dir.join("settings.json"),
        &paths.claude.claude_dir.join("settings.json"),
    )
}

#[tauri::command]
fn install_hook(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    tool: ToolId,
) -> Result<HookStatus, String> {
    let paths = state.collectors.paths();
    let settings_path = match tool {
        ToolId::Antigravity => paths.antigravity.cli_dir.join("settings.json"),
        ToolId::Claude => paths.claude.claude_dir.join("settings.json"),
        ToolId::Codex => return Err("Codex는 별도 Hook 설치가 필요하지 않습니다.".into()),
    };
    install_hook_for_tool(&paths.home, tool, &settings_path)?;
    state.collectors.refresh_live_now(&app, &state.animator);
    Ok(read_hook_status(
        &paths.home,
        &paths.antigravity.cli_dir.join("settings.json"),
        &paths.claude.claude_dir.join("settings.json"),
    ))
}

#[tauri::command]
fn uninstall_hook(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    tool: ToolId,
) -> Result<HookStatus, String> {
    let paths = state.collectors.paths();
    let settings_path = match tool {
        ToolId::Antigravity => paths.antigravity.cli_dir.join("settings.json"),
        ToolId::Claude => paths.claude.claude_dir.join("settings.json"),
        ToolId::Codex => return Ok(get_hook_status(state)),
    };
    uninstall_hook_for_tool(tool, &settings_path)?;
    state.collectors.refresh_live_now(&app, &state.animator);
    Ok(read_hook_status(
        &paths.home,
        &paths.antigravity.cli_dir.join("settings.json"),
        &paths.claude.claude_dir.join("settings.json"),
    ))
}

#[tauri::command]
fn get_autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable().map_err(|e| e.to_string())?;
    } else {
        mgr.disable().map_err(|e| e.to_string())?;
    }
    mgr.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn resize_popover(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    height: f64,
) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let clamped = height.clamp(190.0, 620.0).ceil();
    let _ = window.set_size(Size::Logical(LogicalSize::new(POPOVER_WIDTH, clamped)));
    if let Ok(guard) = state.last_tray_rect.lock() {
        if let Some(rect) = *guard {
            position_window_near_rect(&window, rect);
        }
    }
}

#[tauri::command]
fn quit_app(app: AppHandle, state: tauri::State<'_, AppState>) {
    state.animator.stop();
    app.exit(0);
}

fn toggle_popover_near_tray(app: &AppHandle, tray_rect: Rect) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut guard) = state.last_tray_rect.lock() {
            *guard = Some(tray_rect);
        }
    }

    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    position_window_near_rect(&window, tray_rect);
    let _ = window.show();
    let _ = window.set_focus();
}

fn position_window_near_rect(window: &WebviewWindow, tray_rect: Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let win_size = window
        .outer_size()
        .unwrap_or(tauri::PhysicalSize::new((POPOVER_WIDTH * scale) as u32, (480.0 * scale) as u32));

    let rect_pos = tray_rect.position.to_physical::<f64>(scale);
    let rect_size = tray_rect.size.to_physical::<f64>(scale);

    let mut target_x = rect_pos.x + (rect_size.width / 2.0) - (win_size.width as f64 / 2.0);
    let mut target_y = rect_pos.y + rect_size.height + (6.0 * scale);

    if let Ok(Some(monitor)) = window.current_monitor() {
        let mon_pos = monitor.position();
        let mon_size = monitor.size();
        let min_x = mon_pos.x as f64 + 8.0 * scale;
        let max_x = (mon_pos.x + mon_size.width as i32) as f64 - win_size.width as f64 - 8.0 * scale;
        target_x = target_x.clamp(min_x, max_x.max(min_x));

        let bottom_limit = (mon_pos.y + mon_size.height as i32) as f64;
        if target_y + win_size.height as f64 > bottom_limit {
            target_y = (rect_pos.y - win_size.height as f64 - 6.0 * scale).max(mon_pos.y as f64);
        }
    }

    let _ = window.set_position(Position::Physical(PhysicalPosition::new(
        target_x.round() as i32,
        target_y.round() as i32,
    )));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let bank = SpriteBank::load()?;
            let initial_state = MascotState::Idle;
            let initial_icon = bank.frames(initial_state)[0].clone();

            let tray = TrayIconBuilder::with_id("main-tray")
                .icon(initial_icon)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("Toki")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        toggle_popover_near_tray(tray.app_handle(), rect);
                    }
                })
                .build(app)?;

            let animator = AnimatorHandle::start(tray, bank, initial_state);
            let collectors = CollectorManager::start(app.handle().clone(), animator.clone());
            app.manage(AppState {
                animator,
                collectors,
                last_tray_rect: Mutex::new(None),
            });

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    let _ = window.set_effects(
                        EffectsBuilder::new()
                            .effect(Effect::Popover)
                            .state(EffectState::Active)
                            .radius(14.0)
                            .build(),
                    );
                }
                #[cfg(target_os = "windows")]
                {
                    let _ = window.set_effects(
                        EffectsBuilder::new()
                            .effect(Effect::Acrylic)
                            .build(),
                    );
                }

                let win_clone = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = win_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_usage_snapshot,
            get_data_mode,
            set_data_mode,
            get_mock_scenario,
            set_mock_scenario,
            get_config,
            save_config,
            get_hook_status,
            install_hook,
            uninstall_hook,
            get_autostart_enabled,
            set_autostart_enabled,
            resize_popover,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
