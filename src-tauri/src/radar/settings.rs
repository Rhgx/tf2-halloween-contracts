//! The radar's settings, saved as JSON next to the app's other files. The RCON port and password are created on
//! first run and must match the TF2 launch options the app shows.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store;

const DEFAULT_POS_X: f32 = 0.42;
const DEFAULT_POS_Y: f32 = 0.68;
const DEFAULT_SCALE: f32 = 1.0;
const DEFAULT_OPACITY: f32 = 0.78;
pub const DEFAULT_TOGGLE_KEY: u16 = 0x13; // Pause
pub const DEFAULT_EDIT_KEY: u16 = 0x91; // Scroll Lock

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RadarSettings {
    pub enabled: bool,
    pub port: u16,
    pub password: String,
    /// Longest wait, in seconds, before a new gargoyle reaches the demo file (filler commands keep it short).
    pub demo_delay_s: f32,
    pub opacity: f32,
    pub scale: f32,
    /// Panel centre as fractions of the TF2 client area.
    pub pos_x: f32,
    pub pos_y: f32,
    /// Follow any gargoyle, not just yours (for testing with `ent_create tf_halloween_gift_pickup`).
    pub any_gargoyle: bool,
    /// RCON has connected with this port and password, so the launch options are in place.
    pub setup_done: bool,
    /// Windows virtual-key codes of the in-game hotkeys: turn the overlay on and off, start and save edit mode.
    pub toggle_key: u16,
    pub edit_key: u16,
}

impl Default for RadarSettings {
    fn default() -> Self {
        RadarSettings {
            enabled: false,
            port: 21770,
            password: String::new(),
            demo_delay_s: 5.0,
            opacity: DEFAULT_OPACITY,
            scale: DEFAULT_SCALE,
            pos_x: DEFAULT_POS_X,
            pos_y: DEFAULT_POS_Y,
            any_gargoyle: false,
            setup_done: false,
            toggle_key: DEFAULT_TOGGLE_KEY,
            edit_key: DEFAULT_EDIT_KEY,
        }
    }
}

// A missing file means the defaults, and missing fields take their defaults (serde default). A file that cannot
// be read is replaced by the defaults with the reason printed. A password generated here is saved straight away
// so it stays the same on the next launch.
pub fn load(file: &Path) -> RadarSettings {
    let mut settings = match store::read_optional(file) {
        Ok(None) => RadarSettings::default(),
        Ok(Some(text)) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!(
                "{} is not valid radar settings, using defaults: {err}",
                file.display()
            );
            RadarSettings::default()
        }),
        Err(err) => {
            eprintln!("could not read {}, using defaults: {err}", file.display());
            RadarSettings::default()
        }
    };
    settings.clamp();
    if settings.password.is_empty() {
        settings.password = random_password();
        if let Err(err) = settings.save(file) {
            eprintln!("could not save {}: {err}", file.display());
        }
    }
    settings
}

impl RadarSettings {
    pub fn save(&self, file: &Path) -> io::Result<()> {
        store::write_atomic(file, &serde_json::to_string_pretty(self)?)
    }

    pub fn clamp(&mut self) {
        self.demo_delay_s = self.demo_delay_s.clamp(2.0, 20.0);
        self.pos_x = self.pos_x.clamp(0.0, 1.0);
        self.pos_y = self.pos_y.clamp(0.0, 1.0);
        self.scale = self.scale.clamp(0.5, 2.5);
        self.opacity = self.opacity.clamp(0.0, 1.0);
    }

    pub fn reset_layout(&mut self) {
        (self.pos_x, self.pos_y, self.scale, self.opacity) =
            (DEFAULT_POS_X, DEFAULT_POS_Y, DEFAULT_SCALE, DEFAULT_OPACITY);
    }

    /// Launch options that enable TF2's RCON, reachable from this PC only, plus -condebug for match tracking.
    pub fn launch_options(&self) -> String {
        format!(
            "-usercon +ip 0.0.0.0 +hostport {} +net_start +rcon_password {} +sv_rcon_whitelist_address 127.0.0.1 -condebug",
            self.port, self.password
        )
    }
}

/// 24 hex characters from the OS-seeded hasher keys; only has to be unguessable on this PC.
fn random_password() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (0..2u128)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u128(nanos ^ i);
            format!("{:016x}", h.finish())
        })
        .collect::<String>()[..24]
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_the_defaults() -> io::Result<()> {
        let dir = store::scratch_dir("radar-settings-defaults")?;
        let loaded = load(&dir.join("radar.json"));
        assert_eq!(loaded.password.len(), 24, "a password is generated");
        let expected = RadarSettings {
            password: loaded.password.clone(),
            ..RadarSettings::default()
        };
        assert_eq!(loaded, expected);
        std::fs::remove_dir_all(&dir)
    }

    #[test]
    fn password_is_generated_once_and_kept() -> io::Result<()> {
        let dir = store::scratch_dir("radar-settings-password")?;
        let file = dir.join("radar.json");
        let first = load(&file).password;
        assert_eq!(load(&file).password, first);
        std::fs::remove_dir_all(&dir)
    }

    #[test]
    fn clamp_keeps_values_in_range() {
        let mut settings = RadarSettings {
            demo_delay_s: 0.5,
            pos_x: -1.0,
            pos_y: 3.0,
            scale: 9.0,
            opacity: -0.5,
            ..RadarSettings::default()
        };
        settings.clamp();
        let got = (
            settings.demo_delay_s,
            settings.pos_x,
            settings.pos_y,
            settings.scale,
            settings.opacity,
        );
        assert_eq!(got, (2.0, 0.0, 1.0, 2.5, 0.0));
    }

    #[test]
    fn launch_options_end_with_condebug() {
        let settings = RadarSettings {
            password: "secret".to_string(),
            ..RadarSettings::default()
        };
        let options = settings.launch_options();
        assert!(options.ends_with(" -condebug"), "{options}");
        assert!(options.contains("+rcon_password secret"), "{options}");
    }
}
