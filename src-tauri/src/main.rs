#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod game;
mod maps;
#[cfg(windows)]
mod radar;
mod radar_commands;
mod settings;
mod steam;
mod store;
mod tf2;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use commands::NO_TF2;
use game::Game;

// Paths and live state shared by the commands and the game thread. Set up once in setup().
pub struct AppState {
    /// The TF2 folder the search found, before any override.
    pub detected_tf_dir: Option<PathBuf>,
    /// The folder in use. Changes when the user picks another TF2 folder.
    pub tf2: Mutex<Tf2Paths>,
    pub settings_file: PathBuf,
    pub settings: Mutex<settings::Stored>,
    pub contracts_file: PathBuf,
    pub manual_file: PathBuf,
    pub token_file: PathBuf,
    pub backup_file: PathBuf,
    pub game: Mutex<Game>,
    /// Set when the startup restore failed: queueing is refused with this message until a restore succeeds.
    pub queue_blocked: Mutex<Option<String>>,
    pub steam: std::sync::Arc<steam::Steam>,
}

// The TF2 files for the folder in use. All None when no TF2 folder is in use.
pub struct Tf2Paths {
    /// The TF2 install folder, when it was found.
    pub tf_dir: Option<PathBuf>,
    pub criteria_file: Option<PathBuf>,
    pub console_log: Option<PathBuf>,
}

impl Tf2Paths {
    pub fn new(tf_dir: Option<PathBuf>) -> Self {
        Tf2Paths {
            criteria_file: tf_dir
                .as_ref()
                .map(|dir| dir.join("tf").join("casual_criteria.vdf")),
            console_log: tf_dir
                .as_ref()
                .map(|dir| dir.join("tf").join("console.log")),
            tf_dir,
        }
    }
}

// The window is destroyed when it is closed to the tray (a hidden WebView2 keeps about 300 MB), so showing it may
// mean building it again from the window config.
fn show_main(app: &AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let built = app
                .config()
                .app
                .windows
                .iter()
                .find(|config| config.label == "main")
                .ok_or_else(|| tauri::Error::WindowNotFound)
                .and_then(|config| tauri::WebviewWindowBuilder::from_config(app, config)?.build());
            match built {
                Ok(window) => window,
                Err(err) => {
                    eprintln!("failed to open the main window: {err}");
                    return;
                }
            }
        }
    };
    let result = window
        .unminimize()
        .and_then(|_| window.show())
        .and_then(|_| window.set_focus());
    if let Err(err) = result {
        eprintln!("failed to show main window: {err}");
    }
}

fn main() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        // Registered first so a second launch is forwarded before other plugins start.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        // Launches at login with --autostart, so setup keeps the window hidden.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_game,
            commands::set_queue,
            commands::set_manual,
            commands::launch_tf2,
            commands::open_link,
            commands::quit,
            commands::get_settings,
            commands::set_settings,
            commands::choose_tf2_folder,
            commands::use_detected_tf2_folder,
            commands::open_data_folder,
            commands::start_login,
            commands::cancel_login,
            commands::logout,
            commands::sync,
            radar_commands::get_radar,
            radar_commands::set_radar_enabled,
            radar_commands::set_radar_settings,
            radar_commands::move_radar,
            radar_commands::reset_radar_layout,
            radar_commands::capture_radar_key,
            radar_commands::reset_radar_key
        ])
        .setup(|app| {
            let menu = MenuBuilder::new(app)
                .text("show", "Show")
                .text("quit", "Quit")
                .build()?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("default window icon is missing")?;

            TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            let data_dir = store::data_dir();
            // Builds from the Node version kept their data in another folder; copy it over once.
            if !cfg!(debug_assertions) {
                if let Some(old) = store::old_data_dir().filter(|old| old.exists()) {
                    match store::migrate(&old, &data_dir) {
                        Ok(true) => println!(
                            "Copied your saved data from {} to {}.",
                            old.display(),
                            data_dir.display()
                        ),
                        Ok(false) => {}
                        Err(err) => eprintln!(
                            "Could not copy your saved data from {}: {err}",
                            old.display()
                        ),
                    }
                }
            }

            let settings_file = data_dir.join("settings.json");
            let stored = settings::load(&settings_file).unwrap_or_else(|err| {
                eprintln!("{err} The default settings are used until you change one.");
                settings::Stored::default()
            });

            let detected = tf2::dir::find_tf2_dir();
            let (tf_dir, _) = tf2::dir::active_dir(
                stored.tf2_folder_override.as_deref().map(Path::new),
                detected.as_deref(),
            );
            match &tf_dir {
                Some(dir) => println!("TF2 at {}", dir.display()),
                None => println!("{NO_TF2}"),
            }
            let paths = Tf2Paths::new(tf_dir);

            let backup_file = data_dir.join("casual_criteria.backup.json");
            // A backup left by a crash means the game's queue is still ours, so put the original back first
            // (or drop the backup when the queue should stay in the game).
            let blocked = match commands::release_queue(
                paths.criteria_file.as_deref(),
                &backup_file,
                stored.restore_queue_on_quit,
            ) {
                Ok(true) => {
                    println!(
                        "Found a backup from a previous run and restored casual_criteria.vdf."
                    );
                    None
                }
                Ok(false) => None,
                Err(err) => {
                    eprintln!("{err}");
                    Some(err)
                }
            };

            // The window stays hidden when started by the autostart entry or when set to start minimized.
            let autostart = std::env::args().any(|arg| arg == "--autostart");
            let show_window = !(autostart || stored.start_minimized);

            app.manage(AppState {
                game: Mutex::new(Game {
                    tf2_found: paths.tf_dir.is_some(),
                    ..Game::default()
                }),
                queue_blocked: Mutex::new(blocked),
                steam: std::sync::Arc::new(steam::Steam::new(
                    data_dir.join("steam-token.txt"),
                    data_dir.join("contracts.json"),
                    data_dir.join("manual.json"),
                )),
                contracts_file: data_dir.join("contracts.json"),
                manual_file: data_dir.join("manual.json"),
                token_file: data_dir.join("steam-token.txt"),
                backup_file,
                detected_tf_dir: detected,
                tf2: Mutex::new(paths),
                settings_file,
                settings: Mutex::new(stored),
            });
            game::start(app.handle().clone());
            #[cfg(windows)]
            {
                let radar = radar::runner::Radar::load(&data_dir.join("radar.json"));
                if radar.enabled() {
                    radar.start();
                }
                app.manage(radar);
            }
            if show_window {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let app = window.app_handle();
                    let close_to_tray = app
                        .state::<AppState>()
                        .settings
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .close_to_tray;
                    if !close_to_tray {
                        // The same exit as the tray's Quit, so the queue is released on the way out.
                        api.prevent_close();
                        app.exit(0);
                    }
                    // Otherwise the window closes for real; the app keeps running in the tray (see ExitRequested).
                }
            }
        })
        .build(tauri::generate_context!())?;

    app.run(|app, event| match event {
        // Closing the last window asks to exit without a code; the app stays in the tray instead. Quit and
        // app.exit pass a code and go through.
        RunEvent::ExitRequested {
            api, code: None, ..
        } => api.prevent_exit(),
        // Every way out of the app passes here, including the tray's Quit. A restore already done by `quit` finds no backup.
        RunEvent::Exit => {
            // the radar's threads first, so nothing talks to TF2 while the queue is put back
            #[cfg(windows)]
            app.state::<radar::runner::Radar>().stop();
            commands::restore_on_close(&app.state::<AppState>());
        }
        _ => {}
    });
    Ok(())
}
