import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { LogTail, readLogLine, type MatchState } from './tf2-log.ts';

function play(lines: string[], state: MatchState = { queued: false, inMatch: false, map: null }) {
  const ended = lines.map((line) => readLogLine(line, state)).filter((map) => map !== null);
  return { state, ended };
}

test('queue, match, map change and leaving', () => {
  const { state, ended } = play([
    '[PartyClient] Entering queue for match group 12v12 Casual Match',
    'Lobby created',
    '[PartyClient] Leaving queue for match group 12v12 Casual Match',
    'Map: koth_harvest_event',
    'Map: koth_harvest_event',
    'Map: cp_holyhell',
    'Lobby destroyed'
  ]);
  assert.deepEqual(state, { queued: false, inMatch: false, map: null });
  assert.deepEqual(ended, ['koth_harvest_event', 'cp_holyhell']);
});

test('a Map line outside a lobby is ignored', () => {
  const { state } = play(['Map: itemtest']);
  assert.equal(state.map, null);
});

test('tail reads only complete lines, survives truncation and split characters', () => {
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'tf2-log-')), 'console.log');
  fs.writeFileSync(file, 'old line\nLobby created\nMap: cp_a\n');
  // Keeping the last 20 bytes starts inside "Lobby created"; that fragment must not surface.
  const tail = new LogTail(file, 20);
  assert.deepEqual(tail.read(), ['Map: cp_a']);
  assert.equal(tail.found, true);

  // A line cut mid-write, including in the middle of a multibyte character.
  const emoji = Buffer.from('Map: cp_b 🎃\r\n');
  fs.appendFileSync(file, emoji.subarray(0, 12));
  assert.deepEqual(tail.read(), []);
  fs.appendFileSync(file, emoji.subarray(12));
  assert.deepEqual(tail.read(), ['Map: cp_b 🎃']);

  // TF2 replaced the file with a shorter one.
  fs.writeFileSync(file, 'Lobby destroyed\n');
  assert.deepEqual(tail.read(), ['Lobby destroyed']);

  fs.rmSync(file);
  assert.deepEqual(tail.read(), []);
  assert.equal(tail.found, false);
});
