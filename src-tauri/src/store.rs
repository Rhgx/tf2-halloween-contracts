// The app's own files: synced contracts, manual marks and the Steam token. Every function takes the
// file path so tests can use a temp folder; data_dir() gives the real one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::maps;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contracts {
    pub completed: Vec<String>,
    pub synced_at: String,
}

const DATA_FOLDER: &str = "tf2-halloween-utils";
const OLD_DATA_FOLDER: &str = "tf2-halloween-contracts";
// Files copied by migrate(). The casual criteria backup matters most: it is what puts the game's queue back.
const MIGRATED_FILES: [&str; 4] = [
    "contracts.json",
    "manual.json",
    "steam-token.txt",
    "casual_criteria.backup.json",
];

// Debug builds use the repo's data folder, like the Node server. Release builds use the user's config folder.
pub fn data_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data");
    }
    // like the Node build: next to the exe when there is no config folder
    user_config_root()
        .map(|root| root.join(DATA_FOLDER))
        .or_else(|| {
            std::env::current_exe()
                .ok()?
                .parent()
                .map(Path::to_path_buf)
        })
        .unwrap_or_default()
}

// The folder the Node build used when packaged, for migrate().
pub fn old_data_dir() -> Option<PathBuf> {
    user_config_root().map(|root| root.join(OLD_DATA_FOLDER))
}

fn user_config_root() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }
}

// Copies the known files from the old folder when the new one does not exist yet. Never deletes
// anything, and a second call does nothing.
pub fn migrate(old_dir: &Path, new_dir: &Path) -> io::Result<bool> {
    if new_dir.exists() {
        return Ok(false);
    }
    // copy into a side folder and rename it at the end, so a failed copy never leaves a half-filled new_dir
    // that would block the next attempt
    let mut staging = new_dir.as_os_str().to_os_string();
    staging.push(".migrating");
    let staging = PathBuf::from(staging);
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    for name in MIGRATED_FILES {
        match fs::copy(old_dir.join(name), staging.join(name)) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    fs::rename(&staging, new_dir)?;
    Ok(true)
}

// Writes to `<file>.tmp` and renames over the file, so a crash never leaves a half-written file.
pub fn write_atomic(file: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = file.as_os_str().to_os_string();
    tmp.push(".tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, file)
}

// None when the file does not exist.
pub fn read_optional(file: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn remove_if_exists(file: &Path) -> io::Result<()> {
    match fs::remove_file(file) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn read_json(file: &Path) -> io::Result<Option<Value>> {
    let Some(text) = read_optional(file)? else {
        return Ok(None);
    };
    serde_json::from_str(&text).map(Some).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not valid JSON: {err}", file.display()),
        )
    })
}

// None when the file is missing or has the wrong shape, as the Node reader did.
pub fn read_contracts(file: &Path) -> io::Result<Option<Contracts>> {
    Ok(read_json(file)?.and_then(|value| serde_json::from_value::<Contracts>(value).ok()))
}

pub fn write_contracts(file: &Path, contracts: &Contracts) -> io::Result<()> {
    write_atomic(file, &serde_json::to_string_pretty(contracts)?)
}

// Contracts marked completed by hand, e.g. finished mid-session while TF2 blocks syncing.
pub fn read_manual(file: &Path) -> io::Result<Vec<String>> {
    let Some(Value::Array(items)) = read_json(file)? else {
        return Ok(Vec::new());
    };
    // Skip codes that are no longer in the map list instead of discarding every mark.
    Ok(items
        .iter()
        .filter_map(Value::as_str)
        .filter(|code| maps::by_code(code).is_some())
        .map(str::to_string)
        .collect())
}

pub fn write_manual(file: &Path, codes: &[String]) -> io::Result<()> {
    write_atomic(file, &serde_json::to_string(codes)?)
}

pub fn has_token(file: &Path) -> bool {
    file.exists()
}

pub fn read_token(file: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(file)?.trim().to_string())
}

pub fn save_token(file: &Path, token: &str) -> io::Result<()> {
    write_atomic(file, token)
}

pub fn delete_token(file: &Path) -> io::Result<()> {
    remove_if_exists(file)
}

#[cfg(test)]
pub fn scratch_dir(name: &str) -> io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("tf2-halloween-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_parents_and_leaves_no_tmp_file() -> io::Result<()> {
        let dir = scratch_dir("atomic")?;
        let file = dir.join("nested").join("contracts.json");
        write_atomic(&file, "one")?;
        write_atomic(&file, "two")?;
        assert_eq!(fs::read_to_string(&file)?, "two");
        assert!(!dir.join("nested").join("contracts.json.tmp").exists());
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn contracts_use_the_node_field_names() -> io::Result<()> {
        let dir = scratch_dir("contracts")?;
        let file = dir.join("contracts.json");
        fs::write(
            &file,
            r#"{"completed":["cp_holyhell"],"syncedAt":"2026-10-17T00:00:00.000Z"}"#,
        )?;
        let contracts = read_contracts(&file)?.expect("file exists and has the right shape");
        assert_eq!(contracts.completed, ["cp_holyhell"]);
        assert_eq!(contracts.synced_at, "2026-10-17T00:00:00.000Z");
        write_contracts(&file, &contracts)?;
        assert_eq!(read_contracts(&file)?, Some(contracts));
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn manual_drops_unknown_codes() -> io::Result<()> {
        let dir = scratch_dir("manual")?;
        let file = dir.join("manual.json");
        write_manual(&file, &["cp_holyhell".to_string(), "not_a_map".to_string()])?;
        assert_eq!(read_manual(&file)?, ["cp_holyhell"]);
        assert_eq!(
            read_manual(&dir.join("missing.json"))?,
            Vec::<String>::new()
        );
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn migrate_copies_existing_files_once() -> io::Result<()> {
        let dir = scratch_dir("migrate")?;
        let old = dir.join("old");
        let new = dir.join("new");
        fs::create_dir_all(&old)?;
        fs::write(old.join("contracts.json"), "{}")?;
        fs::write(old.join("steam-token.txt"), "token")?;
        fs::write(old.join("unrelated.txt"), "keep")?;

        assert!(migrate(&old, &new)?);
        assert_eq!(fs::read_to_string(new.join("contracts.json"))?, "{}");
        assert_eq!(fs::read_to_string(new.join("steam-token.txt"))?, "token");
        assert!(!new.join("manual.json").exists());
        assert!(!new.join("unrelated.txt").exists());
        assert!(
            old.join("contracts.json").exists(),
            "the old folder is never deleted"
        );

        fs::write(old.join("manual.json"), "[]")?;
        assert!(!migrate(&old, &new)?);
        assert!(!new.join("manual.json").exists());
        fs::remove_dir_all(&dir)
    }
}
