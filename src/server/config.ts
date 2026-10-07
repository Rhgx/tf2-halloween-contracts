// Where things are. As a packaged exe the UI is embedded and user data lives in %APPDATA%; from
// source the UI is read from dist/ and user data sits in the repo's data/ folder.

import path from 'node:path';
import sea from 'node:sea';

import { findTf2Dir } from '../lib/tf2-dir.ts';

export const TF2_APPID = 440;
export const PORT = Number(process.env.PORT ?? 2715);
export const PACKAGED = sea.isSea();

const ROOT = PACKAGED ? path.dirname(process.execPath) : path.resolve(import.meta.dirname, '../..');
export const DIST = path.join(ROOT, 'dist');

const DATA = PACKAGED ? path.join(process.env.APPDATA ?? ROOT, 'tf2-halloween-contracts') : path.join(ROOT, 'data');
export const BACKUP_FILE = path.join(DATA, 'casual_criteria.backup.json');
export const CONTRACTS_FILE = path.join(DATA, 'contracts.json');
export const MANUAL_FILE = path.join(DATA, 'manual.json');
export const TOKEN_FILE = path.join(DATA, 'steam-token.txt');

export const TF2_DIR = process.env.TF2_DIR ?? findTf2Dir();
export const CRITERIA_FILE = TF2_DIR && path.join(TF2_DIR, 'tf', 'casual_criteria.vdf');
export const CONSOLE_LOG = TF2_DIR && path.join(TF2_DIR, 'tf', 'console.log');
export const NO_TF2 = 'TF2 was not found in your Steam libraries. Set TF2_DIR to its folder and restart the app.';
