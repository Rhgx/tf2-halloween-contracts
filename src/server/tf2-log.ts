// Follows TF2 through tf/console.log, which the game appends to when launched with -condebug.
// Matchmaking logs:
//   [PartyClient] Entering queue for match group 12v12 Casual Match   (and "Leaving queue ...")
//   Lobby created, then "Map: <code>" (again after every map change), then Lobby destroyed on leaving.
// Contract progress is never logged, so a finished match can only offer to mark its contract.

import fs from 'node:fs';

export type MatchState = { queued: boolean; inMatch: boolean; map: string | null };

// Applies one log line to the state. Returns the map whose match just ended, if any.
export function readLogLine(line: string, state: MatchState): string | null {
  if (line.startsWith('[PartyClient] Entering queue')) state.queued = true;
  else if (line.startsWith('[PartyClient] Leaving queue')) state.queued = false;
  else if (line === 'Lobby created') state.inMatch = true;
  else if (line === 'Lobby destroyed') {
    state.inMatch = false;
    return leaveMap(state);
  } else if (line.startsWith('Map: ') && state.inMatch) {
    const map = line.slice(5).trim();
    if (map === state.map) return null;
    const ended = leaveMap(state);
    state.map = map;
    return ended;
  }
  return null;
}

export function leaveMap(state: MatchState) {
  const ended = state.map;
  state.map = null;
  return ended;
}

// Reads complete lines appended to a file since the last call, starting `keepLast` bytes before
// the end so the first read catches up on recent history without replaying the whole file.
export class LogTail {
  found = false;
  private file: string;
  private keepLast: number;
  private offset: number | null = null;
  private rest = Buffer.alloc(0);
  // Starting mid-file lands inside a line; that fragment is dropped.
  private midLine = false;

  constructor(file: string, keepLast = 0) {
    this.file = file;
    this.keepLast = keepLast;
  }

  read(): string[] {
    let size: number;
    try {
      size = fs.statSync(this.file).size;
    } catch {
      this.found = false;
      return [];
    }
    this.found = true;
    if (this.offset === null) {
      this.offset = Math.max(0, size - this.keepLast);
      this.midLine = this.offset > 0;
    }
    if (size < this.offset) {
      // Deleted or replaced; start over.
      this.offset = 0;
      this.rest = Buffer.alloc(0);
    }
    if (size === this.offset) return [];
    const chunk = Buffer.alloc(size - this.offset);
    const fd = fs.openSync(this.file, 'r');
    try {
      fs.readSync(fd, chunk, 0, chunk.length, this.offset);
    } finally {
      fs.closeSync(fd);
    }
    this.offset = size;
    // Split on the last newline byte so a line (or a multibyte character) cut mid-write waits for the rest.
    let data = Buffer.concat([this.rest, chunk]);
    if (this.midLine) {
      const first = data.indexOf(10);
      if (first < 0) {
        this.rest = data;
        return [];
      }
      data = data.subarray(first + 1);
      this.midLine = false;
    }
    const end = data.lastIndexOf(10);
    this.rest = data.subarray(end + 1);
    if (end < 0) return [];
    return data
      .subarray(0, end)
      .toString('utf8')
      .split('\n')
      .map((line) => line.trimEnd());
  }
}
