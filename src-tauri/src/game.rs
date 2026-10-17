// Follows TF2 while it runs: the process list says whether it's running, the console log says where
// it is (see tf2/log.rs). Halloween matches that end are remembered for the Now playing panel, and
// the log and the radar demo are tidied up once TF2 closes.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::maps;
use crate::store;
use crate::tf2::log::{self, LogTail, MatchState};
use crate::tf2::process;
use crate::AppState;

const POLL: Duration = Duration::from_secs(2);
// Catch up on the last 1 MB of the log (a few matches' worth) to know where TF2 is right now.
const CATCH_UP_BYTES: u64 = 1_000_000;
// console.log is cut down to its newest 1 MB once it passes 10 MB, and only while TF2 is closed.
const LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;
const LOG_KEEP_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct Played {
    pub code: String,
    /// When the match ended, in milliseconds since the UNIX epoch.
    pub at: u64,
}

#[derive(Default)]
pub struct Game {
    pub tf2_found: bool,
    pub running: bool,
    pub log_found: bool,
    pub match_state: MatchState,
    /// Halloween maps whose match ended while the app ran, most recent first.
    pub played: Vec<Played>,
    /// Why the automatic sync after TF2 closed gave up, until a manual sync succeeds.
    pub auto_sync_error: Option<String>,
}

pub fn start(app: AppHandle) {
    thread::spawn(move || watch(&app));
}

fn watch(app: &AppHandle) {
    // The TF2 folder can change while the app runs, so the paths are read again on every tick.
    let mut tail_file: Option<PathBuf> = None;
    let mut tail: Option<LogTail> = None;
    // Tidy up at startup if TF2 is closed; after that only a running -> stopped change does it.
    let mut startup = true;
    loop {
        let (tf_dir, console_log) = {
            let state = app.state::<AppState>();
            let paths = state.tf2.lock().unwrap_or_else(PoisonError::into_inner);
            (paths.tf_dir.clone(), paths.console_log.clone())
        };
        if console_log != tail_file {
            tail = console_log
                .clone()
                .map(|file| LogTail::new(file, CATCH_UP_BYTES));
            tail_file = console_log;
            // Matches from before the app started or before the folder changed don't prompt, so this read records nothing.
            read_new_log(app, tail.as_mut(), false);
        }
        let checked = process::tf2_running();
        read_new_log(app, tail.as_mut(), true);
        let state = app.state::<AppState>();
        let mut game = state.game.lock().unwrap_or_else(PoisonError::into_inner);
        game.tf2_found = tf_dir.is_some();
        let was_running = game.running;
        // A failed process check keeps the last answer rather than report a false exit.
        let running = checked.unwrap_or(was_running);
        if !running {
            // Quitting or crashing mid-match logs no "Lobby destroyed", so the map being played ends here.
            if let Some(code) = log::leave_map(&mut game.match_state) {
                if was_running {
                    record_played(&mut game, code);
                }
            }
            game.match_state.queued = false;
            game.match_state.in_match = false;
        }
        game.running = running;
        drop(game);

        if checked.is_some() {
            if was_running && !running {
                tauri::async_runtime::spawn(auto_sync(app.clone()));
            }
            let tidy = !running && (was_running || startup);
            startup = false;
            if tidy && tf_dir.as_deref().is_some_and(housekeeping) {
                // the trimmed log looks like a new file to the tail: re-read it without recording old matches
                read_new_log(app, tail.as_mut(), false);
            }
        }
        thread::sleep(POLL);
    }
}

// After TF2 closes, Steam needs a moment to notice the game session ended: try three times, waiting 10, 20 and
// 30 s, and stop if TF2 starts again. Only when signed in.
async fn auto_sync(app: AppHandle) {
    let steam = app.state::<AppState>().steam.clone();
    for attempt in 1..=3u64 {
        if !store::has_token(&app.state::<AppState>().token_file) {
            return;
        }
        tokio::time::sleep(Duration::from_secs(10 * attempt)).await;
        if process::tf2_running() == Some(true) {
            return;
        }
        match steam.clone().sync().await {
            Ok(()) => return,
            Err(err) if attempt == 3 => {
                let state = app.state::<AppState>();
                state
                    .game
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .auto_sync_error = Some(err);
            }
            Err(_) => {}
        }
    }
}

// Applies whatever TF2 appended to console.log since the last call. `record` is off while catching up.
fn read_new_log(app: &AppHandle, tail: Option<&mut LogTail>, record: bool) {
    let (lines, found) = match tail {
        Some(tail) => {
            let lines = match tail.read() {
                Ok(lines) => lines,
                Err(err) => {
                    eprintln!("could not read console.log: {err}");
                    Vec::new()
                }
            };
            (lines, tail.found)
        }
        None => (Vec::new(), false),
    };
    let state = app.state::<AppState>();
    let mut game = state.game.lock().unwrap_or_else(PoisonError::into_inner);
    game.log_found = found;
    for line in lines {
        if let Some(code) = log::read_log_line(&line, &mut game.match_state) {
            if record {
                record_played(&mut game, code);
            }
        }
    }
}

// Most recent first, one entry per map, and only for maps in the Halloween list.
fn record_played(game: &mut Game, code: String) {
    if maps::by_code(&code).is_none() {
        return;
    }
    game.played.retain(|played| played.code != code);
    game.played.insert(0, Played { code, at: now_ms() });
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

// Runs only while TF2 is closed. Both files are in the tf folder of the TF2 install. Returns whether
// console.log was trimmed.
fn housekeeping(tf_dir: &Path) -> bool {
    let tf = tf_dir.join("tf");
    let trimmed = match log::trim_log(&tf.join("console.log"), LOG_MAX_BYTES, LOG_KEEP_BYTES) {
        Ok(trimmed) => trimmed,
        Err(err) => {
            if err.kind() != io::ErrorKind::NotFound {
                eprintln!("could not trim console.log: {err}");
            }
            false
        }
    };
    if let Err(err) = store::remove_if_exists(&tf.join("gargoyle_radar.dem")) {
        eprintln!("could not remove gargoyle_radar.dem: {err}");
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn played_is_newest_first_without_repeats_or_unknown_maps() {
        let mut game = Game::default();
        record_played(&mut game, "cp_holyhell".to_string());
        record_played(&mut game, "not_a_map".to_string());
        record_played(&mut game, "koth_harvest_event".to_string());
        record_played(&mut game, "cp_holyhell".to_string());
        let codes: Vec<&str> = game
            .played
            .iter()
            .map(|played| played.code.as_str())
            .collect();
        assert_eq!(codes, ["cp_holyhell", "koth_harvest_event"]);
    }
}
