// Follows TF2 while it runs: tasklist says whether it's running, the console log (see tf2-log.ts)
// says where it is. Once TF2 closes, contracts sync by themselves, since the GC can't be read while
// the game holds the session.

import { execFile } from 'node:child_process';

import { MAP_BY_CODE } from '../maps.ts';
import { CONSOLE_LOG, TF2_DIR } from './config.ts';
import { isSyncing, runSync } from './steam.ts';
import { hasToken, readContracts } from './store.ts';
import { LogTail, leaveMap, readLogLine } from './tf2-log.ts';

export const game = {
  tf2Found: TF2_DIR !== null,
  running: false,
  logFound: false,
  queued: false,
  inMatch: false,
  map: null as string | null,
  /** Halloween maps whose match ended while the app ran, most recent first, with when it ended. */
  played: [] as { code: string; at: number }[],
  autoSyncError: null as string | null
};

// Catch up on the last 1 MB of the log (a few matches' worth) to know where TF2 is right now.
const log = CONSOLE_LOG ? new LogTail(CONSOLE_LOG, 1_000_000) : null;

function recordPlayed(map: string) {
  if (MAP_BY_CODE.has(map))
    game.played = [{ code: map, at: Date.now() }, ...game.played.filter((match) => match.code !== map)];
}

// Reads whatever TF2 appended since the last call. `record` is off while catching up on old lines
// at startup, so matches from before the app started don't prompt.
function readNewLog(record: boolean) {
  if (!log) return;
  for (const line of log.read()) {
    const ended = readLogLine(line, game);
    if (ended && record) recordPlayed(ended);
  }
  game.logFound = log.found;
}

function isTf2Running() {
  return new Promise<boolean>((resolve) =>
    execFile('tasklist', ['/NH', '/FO', 'CSV'], { windowsHide: true }, (err, stdout) =>
      // On a failed check keep the last answer rather than report a false exit.
      resolve(err ? game.running : /^"tf(_win64)?\.exe"/im.test(stdout))
    )
  );
}

// Steam takes a moment to notice TF2 closed and refuses the sync until then, so retry a few times.
function autoSync(attempt = 1) {
  if (!hasToken()) return;
  setTimeout(async () => {
    if (game.running) return;
    try {
      await runSync();
      game.autoSyncError = null;
    } catch (err) {
      if (attempt < 3) autoSync(attempt + 1);
      else game.autoSyncError = err instanceof Error ? err.message : String(err);
    }
  }, 10_000 * attempt);
}

async function poll() {
  try {
    const running = await isTf2Running();
    readNewLog(true);
    if (!running) {
      // Quitting or crashing mid-match logs no "Lobby destroyed".
      const ended = leaveMap(game);
      if (ended && game.running) recordPlayed(ended);
      game.queued = game.inMatch = false;
    }
    if (game.running && !running) autoSync();
    game.running = running;
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
  }
  setTimeout(poll, 2000);
}

export function watchGame() {
  readNewLog(false);
  poll();
}

export function gameState() {
  return { ...game, syncing: isSyncing(), syncedAt: readContracts()?.syncedAt ?? null };
}

export type GameState = ReturnType<typeof gameState>;
