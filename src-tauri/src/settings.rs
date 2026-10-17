// The settings the app saves in settings.json. startWithWindows is not saved here: the autostart
// plugin is the source of truth for it.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stored {
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub restore_queue_on_quit: bool,
    /// The TF2 folder picked by the user. Used only while it still has a tf folder inside.
    pub tf2_folder_override: Option<String>,
}

impl Default for Stored {
    fn default() -> Self {
        Stored {
            close_to_tray: true,
            start_minimized: false,
            restore_queue_on_quit: true,
            tf2_folder_override: None,
        }
    }
}

// A missing file means the defaults. Missing fields in the file also take their defaults (serde default).
pub fn load(file: &Path) -> io::Result<Stored> {
    match store::read_json(file)? {
        Some(value) => serde_json::from_value(value).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} has settings the app cannot read: {err}.",
                    file.display()
                ),
            )
        }),
        None => Ok(Stored::default()),
    }
}

pub fn save(file: &Path, stored: &Stored) -> io::Result<()> {
    store::write_atomic(file, &serde_json::to_string_pretty(stored)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_file_and_missing_fields_use_the_defaults() -> io::Result<()> {
        let dir = store::scratch_dir("settings-defaults")?;
        let file = dir.join("settings.json");
        assert_eq!(load(&file)?, Stored::default());

        fs::write(&file, r#"{"startMinimized":true}"#)?;
        let loaded = load(&file)?;
        assert!(loaded.start_minimized);
        assert!(loaded.close_to_tray);
        assert!(loaded.restore_queue_on_quit);
        assert_eq!(loaded.tf2_folder_override, None);
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn saved_settings_load_back() -> io::Result<()> {
        let dir = store::scratch_dir("settings-roundtrip")?;
        let file = dir.join("settings.json");
        let stored = Stored {
            close_to_tray: false,
            tf2_folder_override: Some("D:/Games/Team Fortress 2".to_string()),
            ..Stored::default()
        };
        save(&file, &stored)?;
        assert_eq!(load(&file)?, stored);
        fs::remove_dir_all(&dir)
    }
}
