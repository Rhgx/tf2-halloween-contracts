//! Runs the gargoyle radar while it is switched on: an RCON thread (your position, the demo recording and the
//! filler commands), a demo thread (your gargoyle) and an overlay thread (the in-game panel). They share `Live`,
//! which the Radar tab reads through `Radar::view`. Switching off sets one stop flag and joins all three; the
//! overlay thread destroys its own windows on the way out.

use std::collections::VecDeque;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, sleep, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::{
    CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyNameTextW, MapVirtualKeyW, MAPVK_VK_TO_VSC, VK_ADD, VK_DOWN, VK_ESCAPE,
    VK_HOME, VK_LBUTTON, VK_LEFT, VK_NEXT, VK_OEM_MINUS, VK_OEM_PLUS, VK_PRIOR, VK_RIGHT, VK_SHIFT,
    VK_SUBTRACT, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetClientRect,
    GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic,
    PeekMessageW, RegisterClassW, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, GWL_EXSTYLE, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use super::demo::{DemoFinder, DemoFollower};
use super::nav::{self, Reading};
use super::rcon::{self, Pose, Rcon};
use super::render::{self, Painter, GRIP};
use super::settings::{self, RadarSettings};

const IDLE: Duration = Duration::from_millis(100); // overlay loop pace while TF2 is not in front
const POSE_STALE: Duration = Duration::from_millis(1500);
const RECORD_RETRY: Duration = Duration::from_secs(10);
const NOTICE: Duration = Duration::from_secs(2);
const DEMO_NAME: &str = "gargoyle_radar";
/// TF2 holds the demo in memory and writes it out in bursts of about this many bytes (measured: 672 to 704 KB),
/// about once a minute on a quiet local server. Console commands are recorded into the demo too, so harmless
/// filler commands over RCON make the bursts, and with them a new gargoyle, arrive sooner.
const TYPICAL_BURST: f32 = 700_000.0;
const FILLER_MAX: f32 = 300_000.0; // bytes per second
const FILLER_COMMAND: &str = "alias gargoyle_radar_fill";
const EVENTS_KEPT: usize = 8;
/// Keys edit mode itself uses, so they cannot be hotkeys.
const EDIT_MODE_KEYS: [u16; 11] = [
    VK_LEFT,
    VK_UP,
    VK_RIGHT,
    VK_DOWN,
    VK_PRIOR,
    VK_NEXT,
    VK_HOME,
    VK_OEM_PLUS,
    VK_OEM_MINUS,
    VK_ADD,
    VK_SUBTRACT,
];
/// Not offered as hotkeys when capturing: modifiers on their own, the Windows keys and IME keys.
const IGNORED_KEYS: [u16; 17] = [
    0x03, 0x10, 0x11, 0x12, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C, 0x15, 0x17, 0x18, 0x19,
    0x1C,
];
/// What edit mode shows, so the height pin is visible.
const SAMPLE: Reading = Reading {
    angle_deg: 35.0,
    dist_m: 48.0,
    height_m: 4.0,
};

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Link {
    Off,
    Waiting,
    Unreachable,
    WrongPassword,
    Connected,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum Tone {
    Info,
    Good,
    Warn,
    Bad,
    Gargoyle,
}

#[derive(Clone, Serialize)]
struct Event {
    /// Milliseconds since the UNIX epoch.
    at: u64,
    text: String,
    tone: Tone,
}

/// Everything the three threads share, and what the Radar tab shows.
struct Live {
    file: PathBuf,
    settings: RadarSettings,
    /// The `tf` folder of the TF2 that is running (from its window), for the demo files and the fonts.
    tf_dir: Option<PathBuf>,
    tf2_running: bool,
    link: Link,
    pose: Option<(Pose, Instant)>,
    position_hz: Option<u32>,
    /// A recording of a match is being written (none in menus, after disconnecting, or after `stop`).
    in_match: bool,
    gargoyle: Option<[f32; 3]>,
    map: Option<String>,
    demo_file: Option<String>,
    demo_interval_s: Option<f32>,
    demo_grew: Option<Instant>,
    /// Filler bytes per second to send, set by the demo thread.
    filler: f32,
    /// "Move radar" was pressed in the app; edit mode starts when TF2 is next in front.
    edit_requested: bool,
    editing: bool,
    events: VecDeque<Event>,
}

impl Live {
    fn event(&mut self, tone: Tone, text: impl Into<String>) {
        let text = text.into();
        println!("radar: {text}");
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        self.events.push_front(Event { at, text, tone });
        self.events.truncate(EVENTS_KEPT);
    }

    fn set_link(&mut self, link: Link) {
        if self.link == link {
            return;
        }
        self.link = link;
        match link {
            Link::Connected => {
                self.event(Tone::Good, "Connected to TF2 over RCON.");
                if !self.settings.setup_done {
                    self.settings.setup_done = true;
                    self.save();
                }
            }
            Link::WrongPassword => self.event(
                Tone::Bad,
                "TF2 rejected the RCON password: check the launch options.",
            ),
            Link::Unreachable => self.event(
                Tone::Warn,
                "Cannot reach TF2's RCON: add the launch options, then restart TF2.",
            ),
            Link::Waiting | Link::Off => {}
        }
    }

    fn save(&mut self) {
        if let Err(err) = self.settings.save(&self.file) {
            self.event(Tone::Bad, format!("Could not save radar settings: {err}"));
        }
    }
}

fn lock(live: &Mutex<Live>) -> MutexGuard<'_, Live> {
    live.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Running {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

/// The radar as the app holds it (managed Tauri state).
pub struct Radar {
    live: Arc<Mutex<Live>>,
    running: Mutex<Option<Running>>,
}

/// What the Radar tab gets from `get_radar`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarView {
    settings: SettingsView,
    launch_options: String,
    setup_done: bool,
    status: StatusView,
    events: Vec<Event>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    enabled: bool,
    port: u16,
    demo_delay_s: f32,
    opacity: f32,
    scale: f32,
    any_gargoyle: bool,
    toggle_key: KeyView,
    edit_key: KeyView,
}

#[derive(Serialize)]
struct KeyView {
    code: u16,
    name: String,
}

impl KeyView {
    fn new(code: u16) -> KeyView {
        KeyView {
            code,
            name: key_name(code),
        }
    }
}

/// Which hotkey `capture_key` changes.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySlot {
    Toggle,
    Edit,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusView {
    tf2_running: bool,
    link: Link,
    position_hz: Option<u32>,
    demo: Option<DemoView>,
    gargoyle: Option<GargoyleView>,
    reading: Option<ReadingView>,
    editing: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DemoView {
    file: String,
    interval_s: Option<f32>,
    filler_kbps: f32,
}

#[derive(Serialize)]
struct GargoyleView {
    map: Option<String>,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadingView {
    angle_deg: f32,
    dist_m: f32,
    height_m: f32,
}

/// The settings the Radar tab can change. The overlay's position changes in edit mode only.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    port: Option<u16>,
    demo_delay_s: Option<f32>,
    opacity: Option<f32>,
    scale: Option<f32>,
    any_gargoyle: Option<bool>,
}

impl Radar {
    pub fn load(file: &Path) -> Radar {
        let settings = settings::load(file);
        let live = Live {
            file: file.to_path_buf(),
            filler: TYPICAL_BURST / settings.demo_delay_s,
            settings,
            tf_dir: None,
            tf2_running: false,
            link: Link::Off,
            pose: None,
            position_hz: None,
            in_match: false,
            gargoyle: None,
            map: None,
            demo_file: None,
            demo_interval_s: None,
            demo_grew: None,
            edit_requested: false,
            editing: false,
            events: VecDeque::new(),
        };
        Radar {
            live: Arc::new(Mutex::new(live)),
            running: Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        lock(&self.live).settings.enabled
    }

    /// Starts or stops the three threads and remembers the choice. Stopping waits for them (about two seconds
    /// at most, while an RCON call times out), so call it off the main thread.
    pub fn set_enabled(&self, on: bool) {
        {
            let mut live = lock(&self.live);
            if live.settings.enabled != on {
                live.settings.enabled = on;
                live.save();
            }
        }
        if on {
            self.start();
        } else {
            self.stop();
        }
    }

    pub fn start(&self) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if running.is_some() {
            return;
        }
        {
            let mut live = lock(&self.live);
            live.link = Link::Waiting;
            live.event(Tone::Info, "Radar on.");
        }
        let stop = Arc::new(AtomicBool::new(false));
        let spawn = |name: &str, body: fn(Arc<Mutex<Live>>, Arc<AtomicBool>)| {
            let (live, stop) = (self.live.clone(), stop.clone());
            thread::Builder::new()
                .name(name.to_string())
                .spawn(move || body(live, stop))
                .expect("could not start a radar thread")
        };
        let threads = vec![
            spawn("radar-rcon", rcon_worker),
            spawn("radar-demo", demo_worker),
            spawn("radar-overlay", overlay),
        ];
        *running = Some(Running { stop, threads });
    }

    pub fn stop(&self) {
        let Some(run) = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return;
        };
        run.stop.store(true, Ordering::Relaxed);
        for thread in run.threads {
            let _ = thread.join();
        }
        let mut live = lock(&self.live);
        live.link = Link::Off;
        live.pose = None;
        live.position_hz = None;
        live.in_match = false;
        live.gargoyle = None;
        live.demo_file = None;
        live.demo_interval_s = None;
        live.edit_requested = false;
        live.editing = false;
        live.event(Tone::Info, "Radar off.");
    }

    pub fn update(&self, patch: SettingsPatch) {
        let mut live = lock(&self.live);
        let s = &mut live.settings;
        if let Some(port) = patch.port.filter(|port| *port != s.port) {
            // a new port needs new launch options, so setup starts over (the RCON thread reconnects by itself)
            s.port = port;
            s.setup_done = false;
        }
        s.demo_delay_s = patch.demo_delay_s.unwrap_or(s.demo_delay_s);
        s.opacity = patch.opacity.unwrap_or(s.opacity);
        s.scale = patch.scale.unwrap_or(s.scale);
        s.any_gargoyle = patch.any_gargoyle.unwrap_or(s.any_gargoyle);
        s.clamp();
        live.save();
    }

    pub fn request_edit(&self) {
        lock(&self.live).edit_requested = true;
    }

    /// Waits for the next key or mouse button press (up to 10 s) and binds it. Esc cancels and keeps the old key.
    /// Blocks, so call it off the main thread.
    pub fn capture_key(&self, slot: KeySlot) -> Result<(), String> {
        // let go of whatever pressed the Change button first (a click, Enter or Space)
        let released = Instant::now();
        while (3..=0xFEu16).chain([VK_LBUTTON]).any(Keys::down)
            && released.elapsed() < Duration::from_secs(2)
        {
            sleep(Duration::from_millis(10));
        }
        let asked = Instant::now();
        let vk = loop {
            if asked.elapsed() > Duration::from_secs(10) {
                return Err("No key was pressed within 10 seconds.".to_string());
            }
            if let Some(vk) = (3..=0xFEu16).find(|vk| !IGNORED_KEYS.contains(vk) && Keys::down(*vk))
            {
                break vk;
            }
            sleep(Duration::from_millis(10));
        };
        if vk == VK_ESCAPE {
            return Ok(());
        }
        if EDIT_MODE_KEYS.contains(&vk) {
            return Err(format!(
                "{} is used in edit mode. Pick another key.",
                key_name(vk)
            ));
        }
        let mut live = lock(&self.live);
        let other = match slot {
            KeySlot::Toggle => live.settings.edit_key,
            KeySlot::Edit => live.settings.toggle_key,
        };
        if vk == other {
            return Err(format!("{} is already the other hotkey.", key_name(vk)));
        }
        match slot {
            KeySlot::Toggle => live.settings.toggle_key = vk,
            KeySlot::Edit => live.settings.edit_key = vk,
        }
        live.save();
        Ok(())
    }

    /// Puts one hotkey back to its default. If the other hotkey sits on that default, it swaps to this one's key
    /// so the two never collide.
    pub fn reset_key(&self, slot: KeySlot) {
        let mut live = lock(&self.live);
        let s = &mut live.settings;
        let (this, other, default) = match slot {
            KeySlot::Toggle => (
                &mut s.toggle_key,
                &mut s.edit_key,
                settings::DEFAULT_TOGGLE_KEY,
            ),
            KeySlot::Edit => (
                &mut s.edit_key,
                &mut s.toggle_key,
                settings::DEFAULT_EDIT_KEY,
            ),
        };
        if *other == default {
            *other = *this;
        }
        *this = default;
        live.save();
    }

    pub fn reset_layout(&self) {
        let mut live = lock(&self.live);
        live.settings.reset_layout();
        live.save();
    }

    pub fn view(&self) -> RadarView {
        let live = lock(&self.live);
        let s = &live.settings;
        let pose = live
            .pose
            .filter(|(_, t)| t.elapsed() < POSE_STALE)
            .map(|(p, _)| p);
        let gargoyle = live.gargoyle.filter(|_| live.in_match);
        RadarView {
            settings: SettingsView {
                enabled: s.enabled,
                port: s.port,
                demo_delay_s: s.demo_delay_s,
                opacity: s.opacity,
                scale: s.scale,
                any_gargoyle: s.any_gargoyle,
                toggle_key: KeyView::new(s.toggle_key),
                edit_key: KeyView::new(s.edit_key),
            },
            launch_options: s.launch_options(),
            setup_done: s.setup_done,
            status: StatusView {
                tf2_running: live.tf2_running,
                link: live.link,
                position_hz: live.position_hz,
                demo: live.demo_file.clone().map(|file| DemoView {
                    file,
                    interval_s: live.demo_interval_s,
                    filler_kbps: live.filler / 1000.0,
                }),
                gargoyle: gargoyle.map(|[x, y, z]| GargoyleView {
                    map: live.map.clone(),
                    x,
                    y,
                    z,
                }),
                reading: pose.zip(gargoyle).map(|(pose, g)| {
                    let r = nav::reading(&pose, g);
                    ReadingView {
                        angle_deg: r.angle_deg,
                        dist_m: r.dist_m,
                        height_m: r.height_m,
                    }
                }),
                editing: live.editing || live.edit_requested,
            },
            events: live.events.iter().cloned().collect(),
        }
    }
}

/// Sleeps in short steps so a stop is noticed quickly. Returns true when the radar is stopping.
fn nap(stop: &AtomicBool, total: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < total {
        if stop.load(Ordering::Relaxed) {
            return true;
        }
        sleep(Duration::from_millis(50).min(total.saturating_sub(start.elapsed())));
    }
    stop.load(Ordering::Relaxed)
}

/// Waits for the next frame the desktop composes, so the loops run at the screen's refresh rate.
fn wait_for_frame(since: Instant) {
    // DwmFlush can return at once when nothing is being composed; never run faster than 250 per second
    if unsafe { DwmFlush() } < 0 || since.elapsed() < Duration::from_millis(4) {
        sleep(
            Duration::from_millis(4)
                .saturating_sub(since.elapsed())
                .max(Duration::from_millis(1)),
        );
    }
}

/// A readable name for a virtual-key code, for the hotkey settings and the overlay's help text.
pub fn key_name(vk: u16) -> String {
    let fixed = match vk {
        0x04 => "Middle mouse",
        0x05 => "Mouse 4",
        0x06 => "Mouse 5",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x23 => "End",
        0x90 => "Num Lock",
        0x91 => "Scroll Lock",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    match vk {
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        0x60..=0x69 => return format!("Num {}", vk - 0x60),
        _ => {}
    }
    // letters, digits and punctuation: ask Windows for the name on this keyboard layout
    let mut buf = [0u16; 64];
    let len = unsafe {
        let scan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC);
        GetKeyNameTextW((scan << 16) as i32, buf.as_mut_ptr(), buf.len() as i32)
    };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        format!("Key {vk}")
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Polls `getpos` over RCON once per screen refresh, keeps a demo recording going and sends the filler.
fn rcon_worker(live: Arc<Mutex<Live>>, stop: Arc<AtomicBool>) {
    let mut tick = Instant::now();
    let mut link: Option<(Rcon, u16)> = None;
    let mut last_record: Option<Instant> = None;
    let mut filler_owed = 0.0f32;
    let mut last_fill = Instant::now();
    let filler_text = format!("{FILLER_COMMAND} \"{}\"", "x".repeat(480));
    let (mut replies, mut counted_since) = (0u32, Instant::now());
    while !stop.load(Ordering::Relaxed) {
        wait_for_frame(tick);
        tick = Instant::now();
        let (port, password, tf2_running) = {
            let l = lock(&live);
            (l.settings.port, l.settings.password.clone(), l.tf2_running)
        };
        // a changed port means reconnecting
        if link.as_ref().is_some_and(|(_, p)| *p != port) {
            link = None;
        }
        if !tf2_running {
            link = None;
            lock(&live).set_link(Link::Waiting);
            if nap(&stop, Duration::from_millis(500)) {
                break;
            }
            continue;
        }
        if link.is_none() {
            match Rcon::connect(port, &password) {
                Ok(r) => {
                    link = Some((r, port));
                    lock(&live).set_link(Link::Connected);
                }
                Err(e) => {
                    let l = if e.kind() == std::io::ErrorKind::PermissionDenied {
                        Link::WrongPassword
                    } else {
                        Link::Unreachable
                    };
                    lock(&live).set_link(l);
                    if nap(&stop, Duration::from_secs(2)) {
                        break;
                    }
                    continue;
                }
            }
        }
        let Some((r, _)) = link.as_mut() else {
            continue;
        };
        let pose = match r.exec("getpos;getpos 2") {
            Ok(reply) => rcon::parse_pose(&reply),
            Err(_) => {
                link = None;
                lock(&live).set_link(Link::Unreachable);
                continue;
            }
        };
        if pose.is_some() {
            replies += 1;
        }
        let rate = {
            let mut l = lock(&live);
            if let Some(pose) = pose {
                l.pose = Some((pose, Instant::now()));
            }
            if counted_since.elapsed() >= Duration::from_secs(1) {
                l.position_hz = (replies > 0).then_some(replies);
                (replies, counted_since) = (0, Instant::now());
            }
            // filler only while in a server (getpos answers)
            if pose.is_some() {
                l.filler
            } else {
                0.0
            }
        };

        // never catch up on more than a second
        filler_owed = (filler_owed + rate * last_fill.elapsed().as_secs_f32()).min(rate);
        last_fill = Instant::now();
        let mut sent_ok = true;
        // one command per packet: TF2 drops the RCON connection when a packet holds more than about 510 bytes
        while filler_owed >= filler_text.len() as f32 && sent_ok {
            sent_ok = r.fire(&filler_text).is_ok();
            filler_owed -= filler_text.len() as f32;
        }
        if !sent_ok {
            link = None;
            lock(&live).set_link(Link::Unreachable);
            continue;
        }

        let recording = lock(&live)
            .demo_grew
            .is_some_and(|t| t.elapsed() < RECORD_RETRY);
        if !recording && last_record.is_none_or(|t| t.elapsed() >= RECORD_RETRY) {
            // fails harmlessly in the main menu, or with "Already recording." if a demo is running
            let _ = r.exec(&format!("record {DEMO_NAME}"));
            last_record = Some(Instant::now());
        }
    }
}

/// Follows the demo TF2 is writing, publishes your gargoyle and tunes the filler.
fn demo_worker(live: Arc<Mutex<Live>>, stop: Arc<AtomicBool>) {
    let mut follower: Option<DemoFollower> = None;
    let mut caught_up = false;
    let mut last_burst: Option<Instant> = None;
    let mut last_scan: Option<Instant> = None;
    let mut finder = DemoFinder::default();
    let mut had_gargoyle = false;
    while !nap(&stop, Duration::from_millis(250)) {
        let (tf_dir, any_owner, delay_s) = {
            let l = lock(&live);
            (
                l.tf_dir.clone(),
                l.settings.any_gargoyle,
                l.settings.demo_delay_s,
            )
        };
        let Some(tf_dir) = tf_dir else { continue };
        // listing the demo folders opens every .dem file, so only once a second
        let scan = last_scan.is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if scan {
            last_scan = Some(Instant::now());
        }
        let found = if scan {
            finder.active(&tf_dir, Duration::from_secs(30))
        } else {
            follower.as_ref().map(|f| f.path().to_path_buf())
        };
        match found {
            Some(active) if follower.as_ref().is_none_or(|f| f.path() != active) => {
                let name = active
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                lock(&live).event(Tone::Good, format!("Reading the match from demo {name}."));
                follower = Some(DemoFollower::new(&active));
                caught_up = false;
            }
            // nothing has been written for 30 s: not recording, so not in a match
            None if follower.take().is_some() => {
                lock(&live).event(Tone::Info, "The demo stopped growing: not in a match.")
            }
            _ => {}
        }
        let mut burst = 0;
        if let Some(f) = follower.as_mut() {
            f.any_owner = any_owner;
            let before = f.bytes_read();
            if f.poll() {
                burst = f.bytes_read() - before;
            } else {
                follower = None;
            }
        }
        let gargoyle = follower.as_ref().and_then(|f| f.my_gargoyle());
        let mut l = lock(&live);
        if burst > 0 {
            // the first read is the file so far, not one burst
            if caught_up {
                let interval = last_burst.map_or(delay_s, |t| t.elapsed().as_secs_f32().max(0.25));
                // aim for one burst every `delay_s` seconds: add what the match itself does not write (half a
                // step at a time, so it settles instead of swinging)
                l.filler = (l.filler + 0.5 * burst as f32 * (1.0 / delay_s - 1.0 / interval))
                    .clamp(0.0, FILLER_MAX);
                l.demo_interval_s = Some(interval);
            }
            caught_up = true;
            last_burst = Some(Instant::now());
            l.demo_grew = Some(Instant::now());
        }
        l.map = follower.as_ref().and_then(|f| f.map.clone());
        if gargoyle.is_some() != had_gargoyle {
            match gargoyle {
                Some(g) => {
                    let map = l.map.clone().unwrap_or_else(|| "this map".to_string());
                    l.event(
                        Tone::Gargoyle,
                        format!(
                            "Gargoyle at ({:.0}, {:.0}, {:.0}) on {map}.",
                            g[0], g[1], g[2]
                        ),
                    );
                }
                None => l.event(Tone::Warn, "Gargoyle gone."),
            }
            had_gargoyle = gargoyle.is_some();
        }
        l.in_match = follower.as_ref().is_some_and(|f| !f.ended);
        l.gargoyle = gargoyle;
        l.demo_file = follower
            .as_ref()
            .and_then(|f| f.path().file_name())
            .map(|n| n.to_string_lossy().into_owned());
        if follower.is_none() {
            l.demo_interval_s = None;
        }
    }
}

#[derive(Debug)]
struct Tf2 {
    hwnd: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn tf2_window() -> Option<Tf2> {
    unsafe {
        let hwnd = FindWindowW(wide("Valve001").as_ptr(), null());
        if hwnd.is_null() || IsIconic(hwnd) != 0 {
            return None;
        }
        let mut rect: RECT = zeroed();
        let mut origin = POINT { x: 0, y: 0 };
        GetClientRect(hwnd, &mut rect);
        ClientToScreen(hwnd, &mut origin);
        (rect.right >= 64 && rect.bottom >= 64).then_some(Tf2 {
            hwnd,
            x: origin.x,
            y: origin.y,
            w: rect.right,
            h: rect.bottom,
        })
    }
}

/// The `tf` folder next to the running TF2 executable.
fn tf_dir_of(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
                != 0;
        CloseHandle(process);
        ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))?
            .parent()
            .map(|d| d.join("tf"))
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// A never-focused, always-on-top window showing a premultiplied pixmap with per-pixel alpha.
/// Click-through, except the radar panel while editing. Destroyed on drop, on the thread that made it.
struct Layer {
    hwnd: HWND,
    visible: bool,
    /// Last position and size on screen.
    rect: (i32, i32, i32, i32),
    /// Everything the picture on screen was drawn from, to skip redrawing it unchanged.
    drawn: String,
}

impl Layer {
    fn new(class: &[u16]) -> Layer {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            )
        };
        Layer {
            hwnd,
            visible: false,
            rect: (0, 0, 0, 0),
            drawn: String::new(),
        }
    }

    /// True when the picture described by `key` is not already on screen.
    fn stale(&mut self, key: String) -> bool {
        if self.visible && self.drawn == key {
            return false;
        }
        self.drawn = key;
        true
    }

    fn set_click_through(&self, on: bool) {
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let flag = WS_EX_TRANSPARENT as isize;
            SetWindowLongPtrW(
                self.hwnd,
                GWL_EXSTYLE,
                if on { style | flag } else { style & !flag },
            );
        }
    }

    fn show(&mut self, x: i32, y: i32, pm: &Pixmap) {
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        self.rect = (x, y, w, h);
        unsafe {
            let screen = GetDC(null_mut());
            let mem = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            };
            let mut bits = null_mut();
            let bmp = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if !bmp.is_null() && !bits.is_null() {
                let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
                for (d, s) in dst
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(pm.data().as_chunks::<4>().0)
                {
                    *d = [s[2], s[1], s[0], s[3]]; // premultiplied RGBA -> BGRA
                }
                let old = SelectObject(mem, bmp);
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let (pos, size, src) =
                    (POINT { x, y }, SIZE { cx: w, cy: h }, POINT { x: 0, y: 0 });
                UpdateLayeredWindow(
                    self.hwnd, screen, &pos, &size, mem, &src, 0, &blend, ULW_ALPHA,
                );
                SelectObject(mem, old);
                DeleteObject(bmp);
            }
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            if !self.visible {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
        }
    }

    /// Moves the window without redrawing it (dragging only changes the position).
    fn move_to(&mut self, x: i32, y: i32) {
        if self.visible && (self.rect.0, self.rect.1) != (x, y) {
            self.rect.0 = x;
            self.rect.1 = y;
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
        }
    }

    fn hide(&mut self) {
        if self.visible {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.visible = false;
        }
    }
}

impl Drop for Layer {
    fn drop(&mut self) {
        if !self.hwnd.is_null() {
            unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

/// Polled keyboard and mouse buttons: presses (edge-detected; bit 0 also catches taps shorter than a frame)
/// and auto-repeat for held keys.
#[derive(Default)]
struct Keys {
    held_since: std::collections::HashMap<u16, Instant>,
}

impl Keys {
    fn down(vk: u16) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
    }

    fn pressed(&mut self, vk: u16) -> bool {
        let state = unsafe { GetAsyncKeyState(vk as i32) } as u16;
        let down = state & 0x8000 != 0;
        let was_down = self.held_since.contains_key(&vk);
        if down && !was_down {
            self.held_since.insert(vk, Instant::now());
        } else if !down {
            self.held_since.remove(&vk);
        }
        (down && !was_down) || (!down && state & 1 != 0)
    }

    /// A press, then every frame while held for more than 0.3 s.
    fn repeating(&mut self, vk: u16) -> bool {
        let first = self.pressed(vk);
        first
            || self
                .held_since
                .get(&vk)
                .is_some_and(|t| t.elapsed() > Duration::from_millis(300))
    }
}

#[derive(Clone, Copy)]
enum Drag {
    /// Cursor offset from the panel centre when the drag started.
    Move { dx: i32, dy: i32 },
    /// The panel's top-left corner stays put; the size follows how far the cursor moved from where it grabbed.
    Resize {
        start: (i32, i32),
        size: (i32, i32),
        scale: f32,
        left: i32,
        top: i32,
    },
}

fn cursor() -> (i32, i32) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// The in-game panel: follows TF2's window, draws the reading, handles the two hotkeys and edit mode.
fn overlay(live: Arc<Mutex<Live>>, stop: Arc<AtomicBool>) {
    let mut painter = match Painter::load(None) {
        Ok(p) => p,
        Err(e) => {
            lock(&live).event(Tone::Bad, format!("The overlay cannot start: {e}"));
            return;
        }
    };
    let class = wide("GargoyleRadarOverlay");
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(null()),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        // fails harmlessly with "class already exists" when the radar is switched on a second time
        RegisterClassW(&wc);
    }
    let (mut radar_win, mut help_win, mut corner_win) =
        (Layer::new(&class), Layer::new(&class), Layer::new(&class));

    let mut keys = Keys::default();
    let mut paused = false;
    let mut editing = false;
    let mut drag: Option<Drag> = None;
    let mut notice: Option<(&str, Instant)> = None;
    let mut had_gargoyle = false;
    let mut fonts_from_tf2 = false;
    let mut help_card: Option<(String, Pixmap)> = None;
    while !stop.load(Ordering::Relaxed) {
        let frame_start = Instant::now();
        unsafe {
            let mut msg: MSG = zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let window = tf2_window();
        {
            let mut l = lock(&live);
            l.tf2_running = window.is_some();
            if let Some(tf) = &window {
                if l.tf_dir.is_none() {
                    l.tf_dir = tf_dir_of(tf.hwnd);
                }
                if !fonts_from_tf2 {
                    if let Some(Ok(p)) = l.tf_dir.as_deref().map(|d| Painter::load(Some(d))) {
                        fonts_from_tf2 = p.tf2_fonts;
                        painter = p;
                    }
                }
            }
        }
        let (toggle_key, edit_key) = {
            let l = lock(&live);
            (l.settings.toggle_key, l.settings.edit_key)
        };
        let Some(tf) = window.filter(|tf| unsafe { GetForegroundWindow() } == tf.hwnd) else {
            radar_win.hide();
            help_win.hide();
            corner_win.hide();
            for vk in [toggle_key, edit_key] {
                keys.pressed(vk); // swallow presses made outside the game
            }
            nap(&stop, IDLE.saturating_sub(frame_start.elapsed()));
            continue;
        };

        if keys.pressed(toggle_key) {
            paused = !paused;
            notice = Some((
                if paused { "Radar off" } else { "Radar on" },
                Instant::now(),
            ));
        }
        let requested = std::mem::take(&mut lock(&live).edit_requested);
        if keys.pressed(edit_key) || (requested && !editing) {
            editing = !editing;
            drag = None;
            radar_win.set_click_through(!editing);
            if !editing {
                let mut l = lock(&live);
                l.settings.clamp();
                l.save();
                notice = Some(("Radar position saved", Instant::now()));
            }
        }

        let s = tf.h as f32 / 1080.0;
        let (settings, link, pose, gargoyle) = {
            let mut l = lock(&live);
            l.editing = editing;
            if editing {
                let k = s * l.settings.scale;
                edit_frame(
                    &mut l.settings,
                    &mut keys,
                    &mut drag,
                    &tf,
                    s,
                    k,
                    radar_win.rect,
                );
            }
            let pose = l
                .pose
                .filter(|(_, t)| t.elapsed() < POSE_STALE)
                .map(|(p, _)| p);
            (
                l.settings.clone(),
                l.link,
                pose,
                l.gargoyle.filter(|_| l.in_match),
            )
        };
        // where the panel goes: centre at (pos_x, pos_y) of the TF2 client area, kept fully inside it
        let k = s * settings.scale;
        let (pw, ph) = render::panel_pixels(k);
        let x = ((settings.pos_x * tf.w as f32 - pw as f32 / 2.0) as i32)
            .clamp(0, (tf.w - pw as i32).max(0));
        let y = ((settings.pos_y * tf.h as f32 - ph as f32 / 2.0) as i32)
            .clamp(0, (tf.h - ph as i32).max(0));

        if had_gargoyle && gargoyle.is_none() {
            notice = Some(("Gargoyle gone", Instant::now()));
        }
        had_gargoyle = gargoyle.is_some();

        let reading = match (editing, paused, pose, gargoyle) {
            (true, ..) => Some(SAMPLE),
            (false, false, Some(pose), Some(g)) => Some(nav::reading(&pose, g)),
            _ => None,
        };
        match reading {
            Some(r) => {
                // redraw only when the picture changes; a drag only moves the window
                let (px, py) = (tf.x + x, tf.y + y);
                if radar_win.stale(format!(
                    "{r:?} {k} {} {editing} {fonts_from_tf2}",
                    settings.opacity
                )) {
                    if let Some(mut pm) = Pixmap::new(pw, ph) {
                        painter.panel(&mut pm, k, settings.opacity, &r, editing);
                        radar_win.show(px, py, &pm);
                    }
                } else {
                    radar_win.move_to(px, py);
                }
                if editing {
                    // the card's text is the slow part to draw, so it is drawn once per size and opacity
                    let card_opacity = settings.opacity.max(0.6);
                    let save_key = key_name(edit_key);
                    let card_key = format!("{s} {card_opacity} {fonts_from_tf2} {save_key}");
                    if help_card.as_ref().is_none_or(|(key, _)| *key != card_key) {
                        help_card = Some((
                            card_key.clone(),
                            painter.help_card(s, card_opacity, &save_key),
                        ));
                    }
                    if let Some((_, card)) = &help_card {
                        let (cw, ch) = (card.width() as i32, card.height() as i32);
                        let below = y + ph as i32 + (8.0 * s) as i32;
                        let cy = if below + ch <= tf.h {
                            below
                        } else {
                            (y - ch - (8.0 * s) as i32).max(0)
                        };
                        let cx = (x + pw as i32 / 2 - cw / 2).clamp(0, (tf.w - cw).max(0));
                        if help_win.stale(card_key) {
                            help_win.show(tf.x + cx, tf.y + cy, card);
                        } else {
                            help_win.move_to(tf.x + cx, tf.y + cy);
                        }
                    }
                } else {
                    help_win.hide();
                }
            }
            None => {
                radar_win.hide();
                help_win.hide();
            }
        }

        // corner text: short notices, otherwise only faults; a blank corner is the normal state
        let fault = match (link, pose) {
            _ if paused => None,
            (Link::Connected, Some(_)) => None,
            (Link::Connected, None) => Some("Radar: waiting for position"),
            _ => Some("Radar: not connected to TF2"),
        };
        let edit_text = format!("Edit mode: {} saves", key_name(edit_key));
        let text = if editing {
            Some(edit_text.as_str())
        } else {
            notice
                .filter(|(_, t)| t.elapsed() < NOTICE)
                .map(|(n, _)| n)
                .or(fault)
        };
        match text {
            Some(text) if corner_win.stale(format!("{text} {fonts_from_tf2} {tf:?}")) => {
                let (cw, ch) = ((420.0 * s) as u32, (40.0 * s) as u32);
                let Some(mut pm) = Pixmap::new(cw, ch) else {
                    continue;
                };
                painter.corner(&mut pm, text, s);
                corner_win.show(
                    tf.x + tf.w - (25.0 * s) as i32 - cw as i32,
                    tf.y + tf.h - (19.0 * s) as i32 - ch as i32,
                    &pm,
                );
            }
            Some(_) => {}
            None => corner_win.hide(),
        }
        wait_for_frame(frame_start);
    }
    lock(&live).editing = false;
    // the three layers drop here, destroying their windows on this thread
}

/// One frame of edit mode: mouse drag to move, corner grip to resize, keyboard fallback.
fn edit_frame(
    settings: &mut RadarSettings,
    keys: &mut Keys,
    drag: &mut Option<Drag>,
    tf: &Tf2,
    s: f32,
    k: f32,
    panel: (i32, i32, i32, i32),
) {
    let (mx, my) = cursor();
    let (px, py, pw, ph) = panel;
    let inside = mx >= px && mx < px + pw && my >= py && my < py + ph;
    let grip = (GRIP + 6.0) * k;
    let on_grip =
        inside && (mx - px) as f32 >= pw as f32 - grip && (my - py) as f32 >= ph as f32 - grip;
    if keys.pressed(VK_LBUTTON) && inside {
        *drag = Some(if on_grip {
            Drag::Resize {
                start: (mx, my),
                size: (pw, ph),
                scale: settings.scale,
                left: px - tf.x,
                top: py - tf.y,
            }
        } else {
            Drag::Move {
                dx: mx - (px + pw / 2),
                dy: my - (py + ph / 2),
            }
        });
    }
    if !Keys::down(VK_LBUTTON) {
        *drag = None;
    }
    match *drag {
        Some(Drag::Move { dx, dy }) => {
            settings.pos_x = (mx - dx - tf.x) as f32 / tf.w as f32;
            settings.pos_y = (my - dy - tf.y) as f32 / tf.h as f32;
        }
        Some(Drag::Resize {
            start,
            size,
            scale,
            left,
            top,
        }) => {
            // the grabbed corner follows the cursor: grow by whichever of width or height grew more, so dragging
            // across or down both work, and never jump on the first frame
            let grow_w = (size.0 + mx - start.0) as f32 / size.0.max(1) as f32;
            let grow_h = (size.1 + my - start.1) as f32 / size.1.max(1) as f32;
            settings.scale = (scale * grow_w.max(grow_h)).clamp(0.5, 2.5);
            // keep the top-left corner where it was: the centre is half the new window size from it
            let (pw, ph) = render::panel_pixels(s * settings.scale);
            settings.pos_x = (left as f32 + pw as f32 / 2.0) / tf.w as f32;
            settings.pos_y = (top as f32 + ph as f32 / 2.0) / tf.h as f32;
        }
        None => {}
    }

    let step = if Keys::down(VK_SHIFT) { 10.0 } else { 1.0 };
    for (vk, dx, dy) in [
        (VK_LEFT, -1.0, 0.0),
        (VK_RIGHT, 1.0, 0.0),
        (VK_UP, 0.0, -1.0),
        (VK_DOWN, 0.0, 1.0),
    ] {
        if keys.repeating(vk) {
            settings.pos_x += dx * step / tf.w as f32;
            settings.pos_y += dy * step / tf.h as f32;
        }
    }
    if keys.repeating(VK_OEM_PLUS) || keys.repeating(VK_ADD) {
        settings.scale *= 1.05;
    }
    if keys.repeating(VK_OEM_MINUS) || keys.repeating(VK_SUBTRACT) {
        settings.scale /= 1.05;
    }
    if keys.repeating(VK_PRIOR) {
        settings.opacity += 0.05;
    }
    if keys.repeating(VK_NEXT) {
        settings.opacity -= 0.05;
    }
    if keys.pressed(VK_HOME) {
        settings.reset_layout();
    }
    settings.clamp();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_names_and_reserved_keys() {
        assert_eq!(key_name(settings::DEFAULT_TOGGLE_KEY), "Pause");
        assert_eq!(key_name(settings::DEFAULT_EDIT_KEY), "Scroll Lock");
        assert_eq!(key_name(0x75), "F6");
        assert_eq!(key_name(0x05), "Mouse 4");
        assert!(!key_name(0x4B).is_empty()); // K, from the keyboard layout
                                             // the defaults must be bindable, and edit mode's own keys must not be
        for vk in [settings::DEFAULT_TOGGLE_KEY, settings::DEFAULT_EDIT_KEY] {
            assert!(!EDIT_MODE_KEYS.contains(&vk) && !IGNORED_KEYS.contains(&vk));
        }
        assert!(EDIT_MODE_KEYS.contains(&VK_HOME));
    }
}
