// The casual queue lives in tf/casual_criteria.vdf, which TF2 loads when you click Restore in the
// Casual map selection. It's protobuf text format: one "selected_maps_bits: N" line per 32 maps, bit
// (index % 32) of word (index / 32) set for each selected master_maps_list index. The original file
// is backed up before the first write and put back when the app closes, or on the next start after
// a crash.

import fs from 'node:fs';

import { MAP_BY_CODE, MAPS } from '../maps.ts';
import { BACKUP_FILE, CRITERIA_FILE, NO_TF2 } from './config.ts';
import { readJson, writeAtomic } from './store.ts';

type Backup = { existed: boolean; content: string };

function readWords() {
  if (!CRITERIA_FILE || !fs.existsSync(CRITERIA_FILE)) return [];
  const text = fs.readFileSync(CRITERIA_FILE, 'utf8');
  return [...text.matchAll(/selected_maps_bits\s*:\s*(\d+)/g)].map((m) => Number(m[1]));
}

export function queuedCodes() {
  const words = readWords();
  return MAPS.filter((map) => ((words[map.mapIndex >> 5] ?? 0) >>> (map.mapIndex & 31)) & 1).map((map) => map.code);
}

function readBackup(): Backup | null {
  const value = readJson(BACKUP_FILE);
  if (typeof value !== 'object' || value === null) return null;
  const { existed, content } = value as Record<string, unknown>;
  return typeof existed === 'boolean' && typeof content === 'string' ? { existed, content } : null;
}

// Puts the original file back. Returns whether there was a backup to restore.
export function restoreBackup() {
  if (!fs.existsSync(BACKUP_FILE)) return false;
  if (!CRITERIA_FILE) throw new Error(`${NO_TF2} A backup of casual_criteria.vdf is waiting in ${BACKUP_FILE}.`);
  const backup = readBackup();
  if (!backup) throw new Error(`${BACKUP_FILE} is unreadable; restore casual_criteria.vdf by hand.`);
  if (backup.existed) writeAtomic(CRITERIA_FILE, backup.content);
  else fs.rmSync(CRITERIA_FILE, { force: true });
  fs.rmSync(BACKUP_FILE);
  return true;
}

// Queue only for the given maps. Keeps at least the original word count so TF2 sees the same shape.
export function writeQueue(codes: string[]) {
  if (!CRITERIA_FILE) throw new Error(NO_TF2);
  if (!fs.existsSync(BACKUP_FILE)) {
    const existed = fs.existsSync(CRITERIA_FILE);
    const backup: Backup = { existed, content: existed ? fs.readFileSync(CRITERIA_FILE, 'utf8') : '' };
    writeAtomic(BACKUP_FILE, JSON.stringify(backup));
  }
  const originalWords = readBackup()?.content.match(/selected_maps_bits/g)?.length ?? 0;
  const indexes = codes.map((code) => MAP_BY_CODE.get(code)!.mapIndex);
  const words: number[] = Array(Math.max(originalWords, ...indexes.map((i) => (i >> 5) + 1))).fill(0);
  for (const index of indexes) words[index >> 5] = (words[index >> 5] | (1 << (index & 31))) >>> 0;
  writeAtomic(CRITERIA_FILE, words.map((word) => `selected_maps_bits: ${word}\n`).join(''));
}
