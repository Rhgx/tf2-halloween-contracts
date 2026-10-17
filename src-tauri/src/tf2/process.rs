// Whether TF2 is running. None when the check itself fails, so callers keep their previous answer.

#[cfg(windows)]
const TF2_PROCESS_NAMES: [&str; 2] = ["tf.exe", "tf_win64.exe"];

// The native 64-bit and older 32-bit builds, and the Windows build under Proton. comm is truncated to
// 15 characters by the kernel, which none of these names reach.
#[cfg(target_os = "linux")]
const TF2_PROCESS_NAMES: [&str; 4] = ["tf_linux64", "hl2_linux", "tf.exe", "tf_win64.exe"];

#[cfg(windows)]
pub fn tf2_running() -> Option<bool> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: the snapshot handle is closed on every path, and dwSize is set as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return None;
        }
        let mut found = false;
        loop {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if TF2_PROCESS_NAMES
                .iter()
                .any(|tf2| name.eq_ignore_ascii_case(tf2))
            {
                found = true;
                break;
            }
            if Process32NextW(snapshot, &mut entry) == 0 {
                break;
            }
        }
        CloseHandle(snapshot);
        Some(found)
    }
}

#[cfg(target_os = "linux")]
pub fn tf2_running() -> Option<bool> {
    // Every process name, read from /proc. A process that exits mid-scan is skipped.
    let entries = std::fs::read_dir("/proc").ok()?;
    for entry in entries.flatten() {
        let is_pid = entry
            .file_name()
            .to_str()
            .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()));
        if !is_pid {
            continue;
        }
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        if TF2_PROCESS_NAMES.contains(&comm.trim()) {
            return Some(true);
        }
    }
    Some(false)
}

// TF2 does not run on other platforms, so there is nothing to detect.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn tf2_running() -> Option<bool> {
    None
}
