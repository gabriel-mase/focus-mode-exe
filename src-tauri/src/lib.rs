use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};

mod display;
mod steam;

use display::MonitorInfo;
use steam::Game;

// ── Persistent config ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct Config {
    enabled_exes: HashSet<String>,
    exe_overrides: HashMap<String, String>,
    custom_games: Vec<Game>,
    /// exe_name → list of monitor device_names to disable.
    /// Empty list = default (disable all non-primary).
    game_monitor_configs: HashMap<String, Vec<String>>,
}

fn config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata).join("FocusMode").join("config.json")
}

fn save_config(state: &AppState) {
    let config = Config {
        enabled_exes: state.enabled_exes.clone(),
        exe_overrides: state.exe_overrides.clone(),
        custom_games: state.custom_games.clone(),
        game_monitor_configs: state.game_monitor_configs.clone(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&config) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, json);
    }
}

fn load_config() -> Config {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

// ── App state ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppState {
    pub games: Vec<Game>,
    pub custom_games: Vec<Game>,
    pub enabled_exes: HashSet<String>,
    pub exe_overrides: HashMap<String, String>,
    pub monitors_switched: bool,
    pub game_monitor_configs: HashMap<String, Vec<String>>,
}

type SharedState = Arc<Mutex<AppState>>;

// ── Background monitor loop ────────────────────────────────────────────────────

type SavedConfig = Arc<Mutex<Option<display::SavedDisplayConfig>>>;

fn start_monitor_loop(state: SharedState, saved_cfg: SavedConfig) {
    thread::spawn(move || {
        use sysinfo::System;
        let mut sys = System::new();

        loop {
            // ── Read shared state ──────────────────────────────────────────
            let (enabled_exes, game_monitor_configs, monitors_switched) = {
                let s = state.lock().unwrap();
                (
                    s.enabled_exes.clone(),
                    s.game_monitor_configs.clone(),
                    s.monitors_switched,
                )
            };

            // ── Adaptive sleep ─────────────────────────────────────────────
            // • idle (no games configured)  → 5 s  (barely any CPU impact)
            // • waiting for game to start   → 2 s  (normal polling)
            // • game running                → 500 ms (fast exit detection)
            let sleep_ms: u64 = if enabled_exes.is_empty() {
                5000
            } else if monitors_switched {
                500
            } else {
                2000
            };

            thread::sleep(Duration::from_millis(sleep_ms));

            if enabled_exes.is_empty() {
                continue;
            }

            sys.refresh_processes();

            // Find the first enabled game exe that is currently running
            let running_exe = sys.processes().values().find_map(|p| {
                let name = p.name().to_lowercase();
                enabled_exes
                    .iter()
                    .find(|e| e.to_lowercase() == name)
                    .cloned()
            });

            match (running_exe, monitors_switched) {
                (Some(exe), false) => {
                    // ── Game just started ──────────────────────────────────
                    let monitors_cfg = game_monitor_configs
                        .get(&exe)
                        .cloned()
                        .unwrap_or_default();

                    if monitors_cfg.is_empty() {
                        // Default path: DisplaySwitch.exe /internal + /extend is reliable.
                        display::disable_non_primary_monitors();
                    } else {
                        // Specific monitors path: capture exact current config before
                        // disabling so we can restore it precisely on game close.
                        // DisplaySwitch.exe /extend is NOT reliable here because it may
                        // extend only the currently-active monitors (missing the disabled one).
                        let snapshot = display::capture_display_config();
                        *saved_cfg.lock().unwrap() = snapshot;
                        display::disable_specific_monitors(&monitors_cfg);
                    }

                    state.lock().unwrap().monitors_switched = true;
                }
                (None, true) => {
                    // ── Game just closed ───────────────────────────────────
                    let snapshot = saved_cfg.lock().unwrap().take();
                    if let Some(cfg) = snapshot {
                        display::restore_saved_config(cfg);
                    } else {
                        display::restore_all_monitors();
                    }
                    state.lock().unwrap().monitors_switched = false;
                }
                _ => {} // no change
            }
        }
    });
}

// ── Tauri commands ─────────────────────────────────────────────────────────────

#[tauri::command]
fn get_games(state: tauri::State<'_, SharedState>) -> Vec<Game> {
    let s = state.lock().unwrap();
    let mut all = s.games.clone();
    all.extend(s.custom_games.clone());
    all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    all
}

#[tauri::command]
fn refresh_games(state: tauri::State<'_, SharedState>) -> Vec<Game> {
    let exe_overrides = state.lock().unwrap().exe_overrides.clone();
    let mut games = steam::find_steam_games();
    for game in &mut games {
        if let Some(p) = exe_overrides.get(&game.app_id) {
            apply_override(game, p);
        }
    }
    let mut s = state.lock().unwrap();
    s.games = games;
    let mut all = s.games.clone();
    all.extend(s.custom_games.clone());
    all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    all
}

#[tauri::command]
fn get_enabled_exes(state: tauri::State<'_, SharedState>) -> Vec<String> {
    state.lock().unwrap().enabled_exes.iter().cloned().collect()
}

#[tauri::command]
fn set_game_enabled(exe_name: String, enabled: bool, state: tauri::State<'_, SharedState>) {
    let mut s = state.lock().unwrap();
    if enabled {
        s.enabled_exes.insert(exe_name);
    } else {
        s.enabled_exes.remove(&exe_name);
    }
    save_config(&s);
}

#[tauri::command]
fn pick_exe_file(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .add_filter("Executable", &["exe"])
        .blocking_pick_file()
        .and_then(|fp| fp.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn set_game_exe(app_id: String, exe_path: String, state: tauri::State<'_, SharedState>) {
    let mut s = state.lock().unwrap();
    s.exe_overrides.insert(app_id.clone(), exe_path.clone());
    let found_in_games = s.games.iter_mut().any(|g| {
        if g.app_id == app_id {
            apply_override(g, &exe_path);
            true
        } else {
            false
        }
    });
    if !found_in_games {
        if let Some(g) = s.custom_games.iter_mut().find(|g| g.app_id == app_id) {
            apply_override(g, &exe_path);
        }
    }
    save_config(&s);
}

#[tauri::command]
fn add_custom_game(name: String, exe_path: String, state: tauri::State<'_, SharedState>) -> Game {
    let exe_name = Path::new(&exe_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let id = format!(
        "custom_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let game = Game {
        app_id: id,
        name,
        install_dir: String::new(),
        exe_name: Some(exe_name),
        exe_path: Some(exe_path),
        is_custom: true,
    };
    let mut s = state.lock().unwrap();
    s.custom_games.push(game.clone());
    save_config(&s);
    game
}

#[tauri::command]
fn remove_custom_game(app_id: String, state: tauri::State<'_, SharedState>) {
    let mut s = state.lock().unwrap();
    let exe_to_remove = s
        .custom_games
        .iter()
        .find(|g| g.app_id == app_id)
        .and_then(|g| g.exe_name.clone());
    s.custom_games.retain(|g| g.app_id != app_id);
    if let Some(exe) = exe_to_remove {
        s.enabled_exes.remove(&exe);
    }
    save_config(&s);
}

/// Returns all connected monitors for the UI to display.
#[tauri::command]
fn get_monitors() -> Vec<MonitorInfo> {
    display::enumerate_monitors()
}

/// Saves per-game monitor config. Empty list = disable all non-primary (default).
#[tauri::command]
fn set_game_monitor_config(
    exe_name: String,
    monitor_device_names: Vec<String>,
    state: tauri::State<'_, SharedState>,
) {
    let mut s = state.lock().unwrap();
    if monitor_device_names.is_empty() {
        s.game_monitor_configs.remove(&exe_name);
    } else {
        s.game_monitor_configs.insert(exe_name, monitor_device_names);
    }
    save_config(&s);
}

/// Returns the saved monitor config for a given exe (empty = default).
#[tauri::command]
fn get_game_monitor_configs(state: tauri::State<'_, SharedState>) -> HashMap<String, Vec<String>> {
    state.lock().unwrap().game_monitor_configs.clone()
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn apply_override(game: &mut Game, exe_path: &str) {
    let exe_name = Path::new(exe_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    game.exe_name = Some(exe_name);
    game.exe_path = Some(exe_path.to_string());
}

// ── App entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config();

    let mut games = steam::find_steam_games();
    for game in &mut games {
        if let Some(p) = config.exe_overrides.get(&game.app_id) {
            apply_override(game, p);
        }
    }

    let state: SharedState = Arc::new(Mutex::new(AppState {
        games,
        custom_games: config.custom_games,
        enabled_exes: config.enabled_exes,
        exe_overrides: config.exe_overrides,
        monitors_switched: false,
        game_monitor_configs: config.game_monitor_configs,
    }));

    let saved_cfg: SavedConfig = Arc::new(Mutex::new(None));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state.clone())
        .setup(move |app| {
            start_monitor_loop(state.clone(), saved_cfg);

            // Restore monitors on clean exit
            let state_exit = state.clone();
            app.on_menu_event(move |_app, _event| {});
            let _ = state_exit; // will be used below

            let window = app.get_webview_window("main").unwrap();
            let win_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = win_clone.hide();
                    api.prevent_close();
                }
            });

            let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Focus Mode")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        // Restore monitors before quitting
                        display::restore_all_monitors();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_games,
            refresh_games,
            get_enabled_exes,
            set_game_enabled,
            pick_exe_file,
            set_game_exe,
            add_custom_game,
            remove_custom_game,
            get_monitors,
            set_game_monitor_config,
            get_game_monitor_configs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
