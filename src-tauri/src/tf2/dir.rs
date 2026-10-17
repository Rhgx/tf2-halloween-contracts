// Finds the TF2 install: every library in Steam's libraryfolders.vdf, then Steam's own folder.
// On Windows Steam is found through the registry, then the default install path; on Linux
// through the usual install paths.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
const DEFAULT_STEAM: &str = "C:/Program Files (x86)/Steam";

pub fn find_tf2_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("TF2_DIR") {
        // An empty override means no TF2, as in the Node version, so the search is skipped.
        return (!dir.is_empty()).then(|| PathBuf::from(dir));
    }
    steam_dirs()
        .into_iter()
        .flat_map(|steam| {
            let mut libraries = library_dirs(&steam);
            libraries.push(steam);
            libraries
        })
        .map(|library| library.join("steamapps/common/Team Fortress 2"))
        .find(|dir| dir.join("tf").exists())
}

// The folder in use: the user's override when it still has a tf folder, else the detected one.
// The bool says whether the override is the one in use.
pub fn active_dir(override_dir: Option<&Path>, detected: Option<&Path>) -> (Option<PathBuf>, bool) {
    match override_dir {
        Some(dir) if dir.join("tf").is_dir() => (Some(dir.to_path_buf()), true),
        _ => (detected.map(Path::to_path_buf), false),
    }
}

#[cfg(windows)]
fn steam_dirs() -> Vec<PathBuf> {
    registry_steam_path()
        .into_iter()
        .chain([PathBuf::from(DEFAULT_STEAM)])
        .collect()
}

#[cfg(not(windows))]
fn steam_dirs() -> Vec<PathBuf> {
    // Native installs (~/.steam/steam usually links to the first), then Flatpak and Snap.
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        ".local/share/Steam",
        ".steam/steam",
        ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        "snap/steam/common/.local/share/Steam",
    ]
    .iter()
    .map(|path| home.join(path))
    .filter(|path| path.exists())
    .collect()
}

#[cfg(windows)]
fn registry_steam_path() -> Option<PathBuf> {
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let subkey = wide("Software\\Valve\\Steam");
    let value = wide("SteamPath");
    let mut bytes = 0u32;
    // SAFETY: the strings are NUL-terminated; a null data pointer asks only for the size.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: buf has room for `bytes` bytes, and the size passed in matches its length.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&buf);
    let path = text.trim_end_matches('\0').trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn library_dirs(steam: &Path) -> Vec<PathBuf> {
    // A missing or unreadable libraryfolders.vdf just means no extra libraries, as in the Node version.
    let Ok(text) = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) else {
        return Vec::new();
    };
    vdf_path_values(&text)
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

#[derive(Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Text(String),
}

// Valve KeyValues text: quoted or bare tokens, { } blocks and // comments. Escapes \\ and \" are
// kept only inside quotes.
fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'{' {
            tokens.push(Token::Open);
            i += 1;
        } else if c == b'}' {
            tokens.push(Token::Close);
            i += 1;
        } else if c == b'"' {
            i += 1;
            let mut text = Vec::new();
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && matches!(bytes.get(i + 1), Some(b'"' | b'\\')) {
                    text.push(bytes[i + 1]);
                    i += 2;
                } else {
                    text.push(bytes[i]);
                    i += 1;
                }
            }
            i += 1;
            tokens.push(Token::Text(String::from_utf8_lossy(&text).into_owned()));
        } else {
            let start = i;
            while i < bytes.len() && !is_bare_end(bytes, i) {
                i += 1;
            }
            tokens.push(Token::Text(
                String::from_utf8_lossy(&bytes[start..i]).into_owned(),
            ));
        }
    }
    tokens
}

fn is_bare_end(bytes: &[u8], i: usize) -> bool {
    let c = bytes[i];
    c.is_ascii_whitespace()
        || c == b'{'
        || c == b'}'
        || c == b'"'
        || (c == b'/' && bytes.get(i + 1) == Some(&b'/'))
}

// Every string value of a "path" key at any depth, e.g. the library folders in libraryfolders.vdf.
// Keys followed by a block (not a string) are skipped.
fn vdf_path_values(text: &str) -> Vec<String> {
    let tokens = tokenize(text);
    let mut paths = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        match (&tokens[i], &tokens[i + 1]) {
            (Token::Text(key), Token::Text(value)) => {
                if key == "path" {
                    paths.push(value.clone());
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_library_path_with_escapes_and_comments() {
        let text = r#"
// Steam's library list
"libraryfolders"
{
    "0"
    {
        "path"      "C:\\Program Files (x86)\\Steam"
        "label"     ""
        "apps"
        {
            "440"   "123456"
        }
    }
    "1"
    {
        "path"      "D:\\Games // not a comment inside quotes"
    }
    "2"
    {
        "path"      E:\SteamLibrary // bare value, comment after
        "label"     "say \"hi\""
    }
    "3"
    {
        "path"
        {
        }
    }
}
"#;
        assert_eq!(
            vdf_path_values(text),
            [
                "C:\\Program Files (x86)\\Steam",
                "D:\\Games // not a comment inside quotes",
                "E:\\SteamLibrary"
            ]
        );
    }

    #[test]
    fn override_wins_only_while_it_has_a_tf_folder() -> std::io::Result<()> {
        let dir = crate::store::scratch_dir("active-dir")?;
        let override_dir = dir.join("override");
        let detected = dir.join("detected");
        std::fs::create_dir_all(override_dir.join("tf"))?;

        assert_eq!(
            active_dir(Some(&override_dir), Some(&detected)),
            (Some(override_dir.clone()), true)
        );
        std::fs::remove_dir_all(&override_dir)?;
        assert_eq!(
            active_dir(Some(&override_dir), Some(&detected)),
            (Some(detected.clone()), false)
        );
        assert_eq!(active_dir(None, None), (None, false));
        std::fs::remove_dir_all(&dir)
    }
}
