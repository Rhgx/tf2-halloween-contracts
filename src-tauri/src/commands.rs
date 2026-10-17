// What the UI calls. The shapes match the JSON the Node server sent, with camelCase field names.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::game::Played;
use crate::maps;
use crate::settings;
use crate::steam::LoginState;
use crate::store::{self, Contracts};
use crate::tf2::{dir, queue};
use crate::{AppState, Tf2Paths};

pub const NO_TF2: &str =
    "TF2 was not found in your Steam libraries. Set TF2_DIR to its folder and restart the app.";
const BAD_CODES: &str = "Expected { codes: string[] } of known map codes.";
const TF2_URL: &str = "steam://rungameid/440";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerState {
    queued: Vec<String>,
    contracts: Option<Contracts>,
    manual: Vec<String>,
    logged_in: bool,
    login: Option<LoginState>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    tf2_found: bool,
    running: bool,
    log_found: bool,
    queued: bool,
    in_match: bool,
    map: Option<String>,
    played: Vec<Played>,
    // Steam sync is not wired up until the Steam phase, so there is never an error to report.
    auto_sync_error: Option<String>,
    syncing: bool,
    synced_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    close_to_tray: bool,
    start_minimized: bool,
    // Read from the autostart plugin, not saved in settings.json.
    start_with_windows: bool,
    restore_queue_on_quit: bool,
    // The folder in use: the override when it is set and valid, else the detected folder.
    tf2_folder: Option<String>,
    tf2_folder_detected: Option<String>,
    tf2_folder_overridden: bool,
    data_folder: String,
    version: String,
    platform: &'static str,
}

// Only the fields the UI sent are changed. Missing fields are left as they were.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    close_to_tray: Option<bool>,
    start_minimized: Option<bool>,
    start_with_windows: Option<bool>,
    restore_queue_on_quit: Option<bool>,
}

fn server_state(state: &AppState) -> Result<ServerState, String> {
    let queued = match active_criteria(state) {
        Some(criteria) => queue::queued_codes(&criteria).map_err(|err| err.to_string())?,
        None => Vec::new(),
    };
    Ok(ServerState {
        queued,
        contracts: store::read_contracts(&state.contracts_file).map_err(|err| err.to_string())?,
        manual: store::read_manual(&state.manual_file).map_err(|err| err.to_string())?,
        logged_in: store::has_token(&state.token_file),
        login: state.steam.login_state(),
    })
}

fn game_state(state: &AppState) -> Result<GameState, String> {
    let contracts = store::read_contracts(&state.contracts_file).map_err(|err| err.to_string())?;
    let game = state.game.lock().unwrap_or_else(PoisonError::into_inner);
    Ok(GameState {
        tf2_found: game.tf2_found,
        running: game.running,
        log_found: game.log_found,
        queued: game.match_state.queued,
        in_match: game.match_state.in_match,
        map: game.match_state.map.clone(),
        played: game.played.clone(),
        auto_sync_error: game.auto_sync_error.clone(),
        syncing: state.steam.is_syncing(),
        synced_at: contracts.map(|contracts| contracts.synced_at),
    })
}

// The criteria file of the folder in use.
fn active_criteria(state: &AppState) -> Option<PathBuf> {
    state
        .tf2
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .criteria_file
        .clone()
}

// Every code must be a known Halloween map.
fn check_codes(codes: &[String]) -> Result<(), String> {
    if codes.iter().all(|code| maps::by_code(code).is_some()) {
        Ok(())
    } else {
        Err(BAD_CODES.to_string())
    }
}

// Puts back the queue saved before the first write. Ok(true) when there was a backup to restore.
pub fn restore_queue(criteria: Option<&Path>, backup: &Path) -> Result<bool, String> {
    let Some(criteria) = criteria else {
        if backup.exists() {
            return Err(format!(
                "{NO_TF2} A backup of casual_criteria.vdf is waiting in {}.",
                backup.display()
            ));
        }
        return Ok(false);
    };
    queue::restore_backup(criteria, backup).map_err(|err| err.to_string())
}

// Ends the app's hold on the game's queue. With restoreQueueOnQuit on, the original file goes back. Otherwise the
// backup is dropped and the app's queue stays; the next queue write takes a fresh backup.
pub fn release_queue(
    criteria: Option<&Path>,
    backup: &Path,
    restore: bool,
) -> Result<bool, String> {
    if restore {
        return restore_queue(criteria, backup);
    }
    queue::discard_backup(backup).map_err(|err| err.to_string())?;
    Ok(false)
}

// Called when the app closes. A failure is printed and the backup stays, so the next start retries.
pub fn restore_on_close(state: &AppState) {
    let restore = state
        .settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .restore_queue_on_quit;
    match release_queue(
        active_criteria(state).as_deref(),
        &state.backup_file,
        restore,
    ) {
        Ok(true) => println!("Restored your original casual_criteria.vdf."),
        Ok(false) => {}
        Err(err) => eprintln!("{err}"),
    }
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> Result<ServerState, String> {
    server_state(&state)
}

#[tauri::command]
pub fn get_game(state: State<'_, AppState>) -> Result<GameState, String> {
    game_state(&state)
}

#[tauri::command]
pub fn set_queue(codes: Vec<String>, state: State<'_, AppState>) -> Result<ServerState, String> {
    check_codes(&codes)?;
    // Held across the restore and the write so two queue changes can't interleave.
    let mut blocked = state
        .queue_blocked
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let criteria_file = active_criteria(&state);
    if blocked.is_some() {
        // The startup restore failed, so the game's queue may still be ours. Try again before writing.
        if let Err(err) = restore_queue(criteria_file.as_deref(), &state.backup_file) {
            *blocked = Some(err.clone());
            return Err(err);
        }
        *blocked = None;
    }
    let criteria = criteria_file.as_deref().ok_or(NO_TF2)?;
    queue::write_queue(criteria, &state.backup_file, &codes).map_err(|err| err.to_string())?;
    server_state(&state)
}

#[tauri::command]
pub fn set_manual(codes: Vec<String>, state: State<'_, AppState>) -> Result<ServerState, String> {
    check_codes(&codes)?;
    store::write_manual(&state.manual_file, &codes).map_err(|err| err.to_string())?;
    server_state(&state)
}

#[tauri::command]
pub fn launch_tf2(app: AppHandle, state: State<'_, AppState>) -> Result<GameState, String> {
    app.opener()
        .open_url(TF2_URL, None::<&str>)
        .map_err(|err| format!("Could not open {TF2_URL}: {err}"))?;
    game_state(&state)
}

// Links in the UI (TF2 Wiki pages, GitHub) open in the default browser. Only https, so a page can't open files
// or other programs through it.
#[tauri::command]
pub fn open_link(url: String, app: AppHandle) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("Not an https link: {url}"));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|err| format!("Could not open {url}: {err}"))
}

#[tauri::command]
pub fn quit(app: AppHandle, state: State<'_, AppState>) {
    restore_on_close(&state);
    app.exit(0);
}

fn settings_view(app: &AppHandle, state: &AppState) -> Result<Settings, String> {
    let stored = state
        .settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let (tf2_folder, overridden) = dir::active_dir(
        stored.tf2_folder_override.as_deref().map(Path::new),
        state.detected_tf_dir.as_deref(),
    );
    Ok(Settings {
        close_to_tray: stored.close_to_tray,
        start_minimized: stored.start_minimized,
        start_with_windows: app
            .autolaunch()
            .is_enabled()
            .map_err(|err| err.to_string())?,
        restore_queue_on_quit: stored.restore_queue_on_quit,
        tf2_folder: tf2_folder.map(|folder| folder.display().to_string()),
        tf2_folder_detected: state
            .detected_tf_dir
            .as_ref()
            .map(|folder| folder.display().to_string()),
        tf2_folder_overridden: overridden,
        data_folder: store::data_dir().display().to_string(),
        version: app.package_info().version.to_string(),
        platform: if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "other"
        },
    })
}

// Switches the TF2 folder in use (None for the detected one). The queue goes back into the old install first, so
// that install keeps its original selection. If that fails, the folder is not changed.
fn change_tf2_folder(state: &AppState, override_dir: Option<PathBuf>) -> Result<(), String> {
    // Held for the whole change, like set_queue, so a queue write can't land between the restore and the switch.
    let mut blocked = state
        .queue_blocked
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut paths = state.tf2.lock().unwrap_or_else(PoisonError::into_inner);
    let (active, _) = dir::active_dir(override_dir.as_deref(), state.detected_tf_dir.as_deref());
    let next = Tf2Paths::new(active);
    let mut stored = state
        .settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    // Leaving an install hands its queue back the same way quitting does; the same install is not a switch.
    if next.criteria_file != paths.criteria_file {
        release_queue(
            paths.criteria_file.as_deref(),
            &state.backup_file,
            stored.restore_queue_on_quit,
        )?;
        *blocked = None;
    }
    let mut updated = stored.clone();
    updated.tf2_folder_override = override_dir.map(|folder| folder.display().to_string());
    settings::save(&state.settings_file, &updated)
        .map_err(|err| format!("Could not save settings: {err}"))?;
    *stored = updated;
    *paths = next;
    Ok(())
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> Result<Settings, String> {
    settings_view(&app, &state)
}

#[tauri::command]
pub fn set_settings(
    patch: SettingsPatch,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    if let Some(enable) = patch.start_with_windows {
        let autolaunch = app.autolaunch();
        let result = if enable {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        result.map_err(|err| err.to_string())?;
    }
    {
        let mut stored = state
            .settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut updated = stored.clone();
        updated.close_to_tray = patch.close_to_tray.unwrap_or(updated.close_to_tray);
        updated.start_minimized = patch.start_minimized.unwrap_or(updated.start_minimized);
        updated.restore_queue_on_quit = patch
            .restore_queue_on_quit
            .unwrap_or(updated.restore_queue_on_quit);
        settings::save(&state.settings_file, &updated)
            .map_err(|err| format!("Could not save settings: {err}"))?;
        *stored = updated;
    }
    settings_view(&app, &state)
}

// The picker blocks until it closes, so this is async: async commands run off the main thread.
#[tauri::command]
pub async fn choose_tf2_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return settings_view(&app, &state);
    };
    let folder = picked.into_path().map_err(|err| err.to_string())?;
    if !folder.join("tf").is_dir() {
        return Err(
            "That folder has no tf folder inside. Pick the Team Fortress 2 folder.".to_string(),
        );
    }
    change_tf2_folder(&state, Some(folder))?;
    settings_view(&app, &state)
}

#[tauri::command]
pub fn use_detected_tf2_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    change_tf2_folder(&state, None)?;
    settings_view(&app, &state)
}

#[tauri::command]
pub fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let folder = store::data_dir();
    // Created here so a fresh install still has a folder to open.
    fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    app.opener()
        .open_path(folder.display().to_string(), None::<&str>)
        .map_err(|err| format!("Could not open {}: {err}", folder.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_without_restore_keeps_the_app_queue() -> std::io::Result<()> {
        let dir = store::scratch_dir("release-queue")?;
        let criteria = dir.join("casual_criteria.vdf");
        let backup = dir.join("casual_criteria.backup.json");
        fs::write(&criteria, "selected_maps_bits: 0\n")?;
        queue::write_queue(&criteria, &backup, &["cp_holyhell".to_string()])?;

        assert_eq!(release_queue(Some(&criteria), &backup, false), Ok(false));
        assert!(!backup.exists());
        assert_eq!(queue::queued_codes(&criteria)?, ["cp_holyhell"]);
        fs::remove_dir_all(&dir)
    }
}

#[tauri::command]
pub async fn start_login(app: AppHandle) -> Result<ServerState, String> {
    let steam = app.state::<AppState>().steam.clone();
    crate::steam::guarded(steam.start_login()).await?;
    server_state(&app.state::<AppState>())
}

#[tauri::command]
pub fn cancel_login(state: State<'_, AppState>) -> Result<ServerState, String> {
    state.steam.cancel_login();
    server_state(&state)
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) -> Result<ServerState, String> {
    state.steam.logout()?;
    server_state(&state)
}

// A manual sync that succeeds also clears the automatic sync's error.
#[tauri::command]
pub async fn sync(app: AppHandle) -> Result<ServerState, String> {
    let steam = app.state::<AppState>().steam.clone();
    steam.sync().await?;
    let state = app.state::<AppState>();
    state
        .game
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .auto_sync_error = None;
    server_state(&state)
}
