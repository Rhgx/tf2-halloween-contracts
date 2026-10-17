// Follows TF2 through tf/console.log, which the game appends to when launched with -condebug.
// Matchmaking logs:
//   [PartyClient] Entering queue for match group 12v12 Casual Match   (and "Leaving queue ...")
//   Lobby created, then "Map: <code>" (again after every map change), then Lobby destroyed on leaving.
// Contract progress is never logged, so a finished match can only offer to mark its contract.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::mem;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq)]
pub struct MatchState {
    pub queued: bool,
    pub in_match: bool,
    pub map: Option<String>,
}

// Applies one log line to the state. Returns the map whose match just ended, if any.
pub fn read_log_line(line: &str, state: &mut MatchState) -> Option<String> {
    if line.starts_with("[PartyClient] Entering queue") {
        state.queued = true;
    } else if line.starts_with("[PartyClient] Leaving queue") {
        state.queued = false;
    } else if line == "Lobby created" {
        state.in_match = true;
    } else if line == "Lobby destroyed" {
        state.in_match = false;
        return leave_map(state);
    } else if let Some(name) = line.strip_prefix("Map: ").filter(|_| state.in_match) {
        let map = name.trim();
        if state.map.as_deref() == Some(map) {
            return None;
        }
        let ended = leave_map(state);
        state.map = Some(map.to_string());
        return ended;
    }
    None
}

pub fn leave_map(state: &mut MatchState) -> Option<String> {
    state.map.take()
}

// Reads complete lines appended to a file since the last call, starting `keep_last` bytes before
// the end so the first read catches up on recent history without replaying the whole file.
pub struct LogTail {
    pub found: bool,
    file: PathBuf,
    keep_last: u64,
    offset: Option<u64>,
    rest: Vec<u8>,
    // Starting mid-file lands inside a line; that fragment is dropped.
    mid_line: bool,
}

impl LogTail {
    pub fn new(file: PathBuf, keep_last: u64) -> Self {
        LogTail {
            found: false,
            file,
            keep_last,
            offset: None,
            rest: Vec::new(),
            mid_line: false,
        }
    }

    pub fn read(&mut self) -> io::Result<Vec<String>> {
        let size = match fs::metadata(&self.file) {
            Ok(meta) => meta.len(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.found = false;
                return Ok(Vec::new());
            }
            Err(err) => return Err(err),
        };
        self.found = true;
        let mut offset = match self.offset {
            Some(offset) => offset,
            None => {
                let start = size.saturating_sub(self.keep_last);
                self.mid_line = start > 0;
                start
            }
        };
        if size < offset {
            // Deleted or replaced; start over.
            offset = 0;
            self.rest.clear();
        }
        self.offset = Some(offset);
        if size == offset {
            return Ok(Vec::new());
        }

        let mut chunk = vec![0u8; (size - offset) as usize];
        let mut file = File::open(&self.file)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut chunk)?;
        self.offset = Some(size);

        let mut data = mem::take(&mut self.rest);
        data.extend_from_slice(&chunk);
        if self.mid_line {
            let Some(first) = data.iter().position(|&b| b == b'\n') else {
                self.rest = data;
                return Ok(Vec::new());
            };
            data.drain(..=first);
            self.mid_line = false;
        }
        // Split on the last newline byte so a line (or a multibyte character) cut mid-write waits for the rest.
        let Some(end) = data.iter().rposition(|&b| b == b'\n') else {
            self.rest = data;
            return Ok(Vec::new());
        };
        self.rest = data.split_off(end + 1);
        let text = String::from_utf8_lossy(&data[..end]);
        Ok(text
            .split('\n')
            .map(|line| line.trim_end().to_string())
            .collect())
    }
}

// console.log only ever grows while TF2 runs with -condebug. Once it is bigger than `max_bytes`, keep only
// the newest `keep_bytes`, starting at a whole line. TF2 appends to the end for every line it writes, so
// this is safe while it runs; LogTail notices the shrink and resyncs.
pub fn trim_log(path: &Path, max_bytes: u64, keep_bytes: u64) -> io::Result<bool> {
    let len = std::fs::metadata(path)?.len();
    if len <= max_bytes {
        return Ok(false);
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(len - keep_bytes.min(len)))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    drop(file);
    let start = tail.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
    std::fs::write(path, &tail[start..])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::scratch_dir;

    fn play(lines: &[&str]) -> (MatchState, Vec<String>) {
        let mut state = MatchState::default();
        let ended = lines
            .iter()
            .filter_map(|line| read_log_line(line, &mut state))
            .collect();
        (state, ended)
    }

    #[test]
    fn queue_match_map_change_and_leaving() {
        let (state, ended) = play(&[
            "[PartyClient] Entering queue for match group 12v12 Casual Match",
            "Lobby created",
            "[PartyClient] Leaving queue for match group 12v12 Casual Match",
            "Map: koth_harvest_event",
            "Map: koth_harvest_event",
            "Map: cp_holyhell",
            "Lobby destroyed",
        ]);
        assert_eq!(state, MatchState::default());
        assert_eq!(ended, ["koth_harvest_event", "cp_holyhell"]);
    }

    #[test]
    fn a_map_line_outside_a_lobby_is_ignored() {
        let (state, _) = play(&["Map: itemtest"]);
        assert_eq!(state.map, None);
    }

    #[test]
    fn tail_reads_only_complete_lines_and_survives_truncation_and_split_characters(
    ) -> io::Result<()> {
        let dir = scratch_dir("log-tail")?;
        let file = dir.join("console.log");
        fs::write(&file, "old line\nLobby created\nMap: cp_a\n")?;
        // Keeping the last 20 bytes starts inside "Lobby created"; that fragment must not surface.
        let mut tail = LogTail::new(file.clone(), 20);
        assert_eq!(tail.read()?, ["Map: cp_a"]);
        assert!(tail.found);

        // A line cut mid-write, including in the middle of a multibyte character.
        let emoji = "Map: cp_b ğŸƒ\r\n".as_bytes();
        append(&file, &emoji[..12])?;
        assert!(tail.read()?.is_empty());
        append(&file, &emoji[12..])?;
        assert_eq!(tail.read()?, ["Map: cp_b ğŸƒ"]);

        // TF2 replaced the file with a shorter one.
        fs::write(&file, "Lobby destroyed\n")?;
        assert_eq!(tail.read()?, ["Lobby destroyed"]);

        fs::remove_file(&file)?;
        assert!(tail.read()?.is_empty());
        assert!(!tail.found);
        fs::remove_dir_all(&dir)
    }

    fn append(file: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(file)?
            .write_all(bytes)
    }

    #[test]
    fn trim_log_keeps_the_newest_whole_lines() -> io::Result<()> {
        let dir = scratch_dir("trim-log")?;
        let path = dir.join("console.log");
        let lines: String = (0..1000).map(|i| format!("line {i:04}\n")).collect();
        fs::write(&path, &lines)?;
        assert!(
            !trim_log(&path, 1_000_000, 100)?,
            "under the limit: untouched"
        );
        assert!(trim_log(&path, 5_000, 100)?);
        let kept = fs::read_to_string(&path)?;
        assert!(
            kept.len() <= 100 && kept.starts_with("line ") && kept.ends_with("line 0999\n"),
            "{kept:?}"
        );
        fs::remove_dir_all(&dir)
    }
}
