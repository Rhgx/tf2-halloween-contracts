// Talking to the Rust backend through Tauri commands. The types mirror the structs in src-tauri/src/commands.rs
// and game.rs.

import { invoke } from '@tauri-apps/api/core';

type LoginState = { status: 'waiting' | 'scanned' | 'failed'; qr: string; error?: string };

export type ServerState = {
  queued: string[];
  contracts: { completed: string[]; syncedAt: string } | null;
  manual: string[];
  loggedIn: boolean;
  // Always null until the Steam phase; the type keeps the sign-in dialog compiling until then.
  login: LoginState | null;
};

export type GameState = {
  tf2Found: boolean;
  running: boolean;
  logFound: boolean;
  queued: boolean;
  inMatch: boolean;
  map: string | null;
  played: { code: string; at: number }[];
  autoSyncError: string | null;
  syncing: boolean;
  syncedAt: string | null;
};

export type Settings = {
  closeToTray: boolean;
  startMinimized: boolean;
  startWithWindows: boolean;
  restoreQueueOnQuit: boolean;
  tf2Folder: string | null;
  tf2FolderDetected: string | null;
  tf2FolderOverridden: boolean;
  dataFolder: string;
  version: string;
  platform: 'windows' | 'linux' | 'other';
};

export type SettingsPatch = Partial<
  Pick<Settings, 'closeToTray' | 'startMinimized' | 'startWithWindows' | 'restoreQueueOnQuit'>
>;

type RadarLink = 'off' | 'waiting' | 'unreachable' | 'wrong-password' | 'connected';

type RadarEvent = { at: number; text: string; tone: 'info' | 'good' | 'warn' | 'bad' | 'gargoyle' };

export type RadarReading = { angleDeg: number; distM: number; heightM: number };

export type RadarKey = { code: number; name: string };

export const DEFAULT_RADAR_KEYS: Record<'toggle' | 'edit', RadarKey> = {
  toggle: { code: 0x13, name: 'Pause' },
  edit: { code: 0x91, name: 'Scroll Lock' }
};

export type Radar = {
  settings: {
    enabled: boolean;
    port: number;
    demoDelayS: number;
    opacity: number;
    scale: number;
    anyGargoyle: boolean;
    /** In-game hotkeys: a Windows virtual-key code and its readable name. */
    toggleKey: RadarKey;
    editKey: RadarKey;
  };
  /** The full launch options line to copy, password and -condebug included. */
  launchOptions: string;
  /** RCON has connected at least once with these options. */
  setupDone: boolean;
  status: {
    tf2Running: boolean;
    /** 'off' while the radar is disabled. */
    link: RadarLink;
    /** getpos replies per second while reading. */
    positionHz: number | null;
    /** The recording being followed; intervalS is null until the write interval is measured. */
    demo: { file: string; intervalS: number | null; fillerKbps: number } | null;
    gargoyle: { map: string | null; x: number; y: number; z: number } | null;
    /** The live arrow, once a gargoyle and a pose exist. */
    reading: RadarReading | null;
    /** Overlay edit mode is on, or pending until TF2 is in front. */
    editing: boolean;
  };
  /** Newest first, at most 8. */
  events: RadarEvent[];
};

export type RadarSettingsPatch = Partial<
  Pick<Radar['settings'], 'port' | 'demoDelayS' | 'opacity' | 'scale' | 'anyGargoyle'>
>;

export function getState() {
  return invoke<ServerState>('get_state');
}

export function getSettings() {
  return invoke<Settings>('get_settings');
}

export function setSettings(patch: SettingsPatch) {
  return invoke<Settings>('set_settings', { patch });
}

// Opens the app's folder picker; resolves with the settings as they are afterwards, also when cancelled.
export function chooseTf2Folder() {
  return invoke<Settings>('choose_tf2_folder');
}

export function useDetectedTf2Folder() {
  return invoke<Settings>('use_detected_tf2_folder');
}

export function openDataFolder() {
  return invoke<void>('open_data_folder');
}

export function getGame() {
  return invoke<GameState>('get_game');
}

export function setQueue(codes: string[]) {
  return invoke<ServerState>('set_queue', { codes });
}

export function setManual(codes: string[]) {
  return invoke<ServerState>('set_manual', { codes });
}

export function launchTf2() {
  return invoke<GameState>('launch_tf2');
}

export function openLink(url: string) {
  return invoke<void>('open_link', { url });
}

export function quit() {
  return invoke<void>('quit');
}

// Null on platforms without the radar (Linux).
export function getRadar() {
  return invoke<Radar | null>('get_radar');
}

export function setRadarEnabled(enabled: boolean) {
  return invoke<Radar>('set_radar_enabled', { enabled });
}

export function setRadarSettings(patch: RadarSettingsPatch) {
  return invoke<Radar>('set_radar_settings', { patch });
}

// Enters overlay edit mode; applies when TF2 is next in front.
export function moveRadar() {
  return invoke<Radar>('move_radar');
}

export function resetRadarLayout() {
  return invoke<Radar>('reset_radar_layout');
}

// Waits up to 10 s for the next key or mouse button pressed anywhere (captured natively, so the window needs no
// focus). Esc cancels and resolves unchanged; a timeout, a key edit mode uses, or the other hotkey's key rejects.
export function captureRadarKey(which: 'toggle' | 'edit') {
  return invoke<Radar>('capture_radar_key', { which });
}

// Puts one hotkey back to its default; if the other hotkey sits on that key, the backend moves it to this one's
// old key so they never collide.
export function resetRadarKey(which: 'toggle' | 'edit') {
  return invoke<Radar>('reset_radar_key', { which });
}

// Steam: QR sign-in (the dialog polls getState while it is pending), sign-out and contract sync.
export function startLogin() {
  return invoke<ServerState>('start_login');
}

export function cancelLogin() {
  return invoke<ServerState>('cancel_login');
}

export function logout() {
  return invoke<ServerState>('logout');
}

export function syncContracts() {
  return invoke<ServerState>('sync');
}

// Tauri rejects with the Rust error string as is; Error objects still give their message.
export function errorMessage(err: unknown) {
  if (typeof err === 'string') return err;
  return err instanceof Error ? err.message : String(err);
}
