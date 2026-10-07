// The app's own files: synced contracts, manual marks and the Steam token.

import fs from 'node:fs';
import path from 'node:path';

import { MAP_BY_CODE } from '../maps.ts';
import { CONTRACTS_FILE, MANUAL_FILE, TOKEN_FILE } from './config.ts';

export type Contracts = { completed: string[]; syncedAt: string };

export function writeAtomic(file: string, content: string) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = `${file}.tmp`;
  fs.writeFileSync(tmp, content);
  fs.renameSync(tmp, file);
}

export function readJson(file: string): unknown {
  return fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : undefined;
}

export function readContracts(): Contracts | null {
  const value = readJson(CONTRACTS_FILE);
  if (typeof value !== 'object' || value === null) return null;
  const { completed, syncedAt } = value as Record<string, unknown>;
  return Array.isArray(completed) && completed.every((c) => typeof c === 'string') && typeof syncedAt === 'string'
    ? { completed, syncedAt }
    : null;
}

export function writeContracts(contracts: Contracts) {
  writeAtomic(CONTRACTS_FILE, JSON.stringify(contracts, null, 2));
}

// Contracts marked completed by hand, e.g. finished mid-session while TF2 blocks syncing.
export function readManual(): string[] {
  const value = readJson(MANUAL_FILE);
  // Skip codes that are no longer in the map list instead of discarding every mark.
  return Array.isArray(value) ? value.filter((code) => typeof code === 'string' && MAP_BY_CODE.has(code)) : [];
}

export function writeManual(codes: string[]) {
  writeAtomic(MANUAL_FILE, JSON.stringify(codes));
}

export const hasToken = () => fs.existsSync(TOKEN_FILE);
export const readToken = () => fs.readFileSync(TOKEN_FILE, 'utf8').trim();
export const saveToken = (token: string) => writeAtomic(TOKEN_FILE, token);
export const deleteToken = () => fs.rmSync(TOKEN_FILE, { force: true });
