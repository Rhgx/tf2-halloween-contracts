// The Radar tab's commands. The radar is Windows only: elsewhere get_radar answers null (the tab stays hidden)
// and the rest refuse.

#[cfg(windows)]
mod imp {
    use tauri::{AppHandle, Manager, State};

    use crate::radar::runner::{KeySlot, Radar, RadarView, SettingsPatch};

    #[tauri::command]
    pub fn get_radar(radar: State<'_, Radar>) -> Option<RadarView> {
        Some(radar.view())
    }

    // Async so stopping (which waits for the radar's threads) runs off the main thread.
    #[tauri::command]
    pub async fn set_radar_enabled(enabled: bool, app: AppHandle) -> Result<RadarView, String> {
        tauri::async_runtime::spawn_blocking(move || {
            let radar = app.state::<Radar>();
            radar.set_enabled(enabled);
            radar.view()
        })
        .await
        .map_err(|err| err.to_string())
    }

    #[tauri::command]
    pub fn set_radar_settings(patch: SettingsPatch, radar: State<'_, Radar>) -> RadarView {
        radar.update(patch);
        radar.view()
    }

    #[tauri::command]
    pub fn move_radar(radar: State<'_, Radar>) -> RadarView {
        radar.request_edit();
        radar.view()
    }

    #[tauri::command]
    pub fn reset_radar_layout(radar: State<'_, Radar>) -> RadarView {
        radar.reset_layout();
        radar.view()
    }

    // Async: it waits up to 10 s for the key press.
    #[tauri::command]
    pub async fn capture_radar_key(which: KeySlot, app: AppHandle) -> Result<RadarView, String> {
        tauri::async_runtime::spawn_blocking(move || {
            let radar = app.state::<Radar>();
            radar.capture_key(which)?;
            Ok(radar.view())
        })
        .await
        .map_err(|err| err.to_string())?
    }

    #[tauri::command]
    pub fn reset_radar_key(which: KeySlot, radar: State<'_, Radar>) -> RadarView {
        radar.reset_key(which);
        radar.view()
    }
}

#[cfg(not(windows))]
mod imp {
    const WINDOWS_ONLY: &str = "The gargoyle radar is Windows only.";

    #[tauri::command]
    pub fn get_radar() -> Option<()> {
        None
    }

    #[tauri::command]
    pub fn set_radar_enabled(_enabled: bool) -> Result<(), String> {
        Err(WINDOWS_ONLY.to_string())
    }

    #[tauri::command]
    pub fn set_radar_settings(_patch: serde_json::Value) -> Result<(), String> {
        Err(WINDOWS_ONLY.to_string())
    }

    #[tauri::command]
    pub fn move_radar() -> Result<(), String> {
        Err(WINDOWS_ONLY.to_string())
    }

    #[tauri::command]
    pub fn reset_radar_layout() -> Result<(), String> {
        Err(WINDOWS_ONLY.to_string())
    }

    #[tauri::command]
    pub fn capture_radar_key(_which: String) -> Result<(), String> {
        Err(WINDOWS_ONLY.to_string())
    }

    #[tauri::command]
    pub fn reset_radar_key(_which: String) -> Result<(), String> {
        Err(WINDOWS_ONLY.to_string())
    }
}

pub use imp::*;
