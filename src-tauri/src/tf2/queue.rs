// The casual queue lives in tf/casual_criteria.vdf, which TF2 loads when you click Restore in the
// Casual map selection. It's protobuf text format: one "selected_maps_bits: N" line per 32 maps, bit
// (index % 32) of word (index / 32) set for each selected master_maps_list index. The original file
// is backed up before the first write and put back when the app closes, or on the next start after
// a crash.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::maps::{self, MAPS};
use crate::store;

const WORD_KEY: &str = "selected_maps_bits";

// Serializes writes and restores of the criteria file and its backup.
static QUEUE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Serialize, Deserialize)]
struct Backup {
    existed: bool,
    content: String,
}

// Reads every `selected_maps_bits: N` value in order. Numbers wrap to 32 bits like the Node version.
fn read_words(text: &str) -> Vec<u32> {
    let mut words = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find(WORD_KEY) {
        rest = &rest[pos + WORD_KEY.len()..];
        let Some(after_colon) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let digits = after_colon.trim_start();
        let len = digits.bytes().take_while(u8::is_ascii_digit).count();
        if let Ok(value) = digits[..len].parse::<u64>() {
            words.push(value as u32);
        }
        rest = &digits[len..];
    }
    words
}

fn map_index(code: &str) -> io::Result<u32> {
    maps::by_code(code).map(|map| map.map_index).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown map code {code}"),
        )
    })
}

fn read_backup(backup_file: &Path) -> io::Result<Option<Backup>> {
    let Some(value) = store::read_json(backup_file)? else {
        return Ok(None);
    };
    Ok(serde_json::from_value(value).ok())
}

// Codes of the maps selected in the criteria file. A missing file means no maps are selected.
pub fn queued_codes(criteria: &Path) -> io::Result<Vec<String>> {
    let text = store::read_optional(criteria)?.unwrap_or_default();
    let words = read_words(&text);
    Ok(MAPS
        .iter()
        .filter(|map| {
            let word = words
                .get((map.map_index >> 5) as usize)
                .copied()
                .unwrap_or(0);
            (word >> (map.map_index & 31)) & 1 == 1
        })
        .map(|map| map.code.clone())
        .collect())
}

// Puts the original file back. Returns whether there was a backup to restore.
pub fn restore_backup(criteria: &Path, backup_file: &Path) -> io::Result<bool> {
    let _guard = QUEUE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    if !backup_file.exists() {
        return Ok(false);
    }
    // The backup is kept when it cannot be read, so the original can still be restored by hand.
    let backup = read_backup(backup_file)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is unreadable; restore casual_criteria.vdf by hand.",
                backup_file.display()
            ),
        )
    })?;
    if backup.existed {
        store::write_atomic(criteria, &backup.content)?;
    } else {
        store::remove_if_exists(criteria)?;
    }
    fs::remove_file(backup_file)?;
    Ok(true)
}

// Drops the backup and leaves the criteria file as it is, so the app's queue stays in the game.
pub fn discard_backup(backup_file: &Path) -> io::Result<()> {
    let _guard = QUEUE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    store::remove_if_exists(backup_file)
}

// Queues only the given maps. Keeps at least the original word count so TF2 sees the same shape.
pub fn write_queue(criteria: &Path, backup_file: &Path, codes: &[String]) -> io::Result<()> {
    let _guard = QUEUE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let indexes = codes
        .iter()
        .map(|code| map_index(code))
        .collect::<io::Result<Vec<u32>>>()?;
    if !backup_file.exists() {
        let existed = criteria.exists();
        let content = if existed {
            fs::read_to_string(criteria)?
        } else {
            String::new()
        };
        let backup = Backup { existed, content };
        store::write_atomic(backup_file, &serde_json::to_string(&backup)?)?;
    }
    let original_words =
        read_backup(backup_file)?.map_or(0, |backup| backup.content.matches(WORD_KEY).count());
    let len = indexes
        .iter()
        .map(|index| (index >> 5) as usize + 1)
        .max()
        .unwrap_or(0)
        .max(original_words);
    let mut words = vec![0u32; len];
    for index in indexes {
        words[(index >> 5) as usize] |= 1 << (index & 31);
    }
    let text: String = words
        .iter()
        .map(|word| format!("{WORD_KEY}: {word}\n"))
        .collect();
    store::write_atomic(criteria, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::scratch_dir;

    fn codes(list: &[&str]) -> Vec<String> {
        list.iter().map(|code| code.to_string()).collect()
    }

    #[test]
    fn written_codes_read_back() -> io::Result<()> {
        let dir = scratch_dir("queue-bits")?;
        let criteria = dir.join("casual_criteria.vdf");
        let backup = dir.join("casual_criteria.backup.json");
        // Nine words, as a real file has, so the written file keeps that shape.
        fs::write(&criteria, "selected_maps_bits: 0\n".repeat(9))?;

        write_queue(
            &criteria,
            &backup,
            &codes(&["koth_harvest_event", "cp_holyhell"]),
        )?;
        // Codes come back in map-list order, as in the Node version.
        assert_eq!(
            queued_codes(&criteria)?,
            ["cp_holyhell", "koth_harvest_event"]
        );
        assert_eq!(fs::read_to_string(&criteria)?.lines().count(), 9);
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn backup_is_made_once_and_restored() -> io::Result<()> {
        let dir = scratch_dir("queue-backup")?;
        let criteria = dir.join("casual_criteria.vdf");
        let backup = dir.join("casual_criteria.backup.json");
        let original = "selected_maps_bits: 5\n";
        fs::write(&criteria, original)?;

        write_queue(&criteria, &backup, &codes(&["cp_holyhell"]))?;
        write_queue(&criteria, &backup, &codes(&["koth_harvest_event"]))?;
        let saved: Backup = serde_json::from_str(&fs::read_to_string(&backup)?)?;
        assert!(saved.existed);
        assert_eq!(
            saved.content, original,
            "the second write must not replace the backup"
        );

        assert!(restore_backup(&criteria, &backup)?);
        assert_eq!(fs::read_to_string(&criteria)?, original);
        assert!(!backup.exists());
        assert!(!restore_backup(&criteria, &backup)?);
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn restore_deletes_a_vdf_that_did_not_exist() -> io::Result<()> {
        let dir = scratch_dir("queue-missing")?;
        let criteria = dir.join("casual_criteria.vdf");
        let backup = dir.join("casual_criteria.backup.json");

        write_queue(&criteria, &backup, &codes(&["cp_holyhell"]))?;
        assert!(criteria.exists());
        assert!(restore_backup(&criteria, &backup)?);
        assert!(!criteria.exists());
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn corrupt_backup_is_an_error_and_is_kept() -> io::Result<()> {
        let dir = scratch_dir("queue-corrupt")?;
        let criteria = dir.join("casual_criteria.vdf");
        let backup = dir.join("casual_criteria.backup.json");
        fs::write(&backup, "not json")?;

        assert!(restore_backup(&criteria, &backup).is_err());
        assert!(backup.exists());
        fs::remove_dir_all(&dir)
    }

    #[test]
    fn restore_without_a_backup_returns_false() -> io::Result<()> {
        let dir = scratch_dir("queue-none")?;
        assert!(!restore_backup(&dir.join("a.vdf"), &dir.join("a.json"))?);
        fs::remove_dir_all(&dir)
    }
}
