// Local companion server for the checklist. Serves the UI on 127.0.0.1 and an /api for it:
// Steam sign-in and contract sync (steam.ts), the casual queue (casual-queue.ts), manual marks
// (store.ts), and where TF2 is right now (game.ts). Closing it puts the original casual queue back.

import { execFile } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import sea from 'node:sea';

import { MAP_BY_CODE } from '../maps.ts';
import { queuedCodes, restoreBackup, writeQueue } from './casual-queue.ts';
import { DIST, NO_TF2, PACKAGED, PORT, TF2_APPID, TF2_DIR } from './config.ts';
import { game, gameState, watchGame } from './game.ts';
import { cancelLogin, loginState, runSync, startLogin } from './steam.ts';
import { deleteToken, hasToken, readContracts, readManual, writeManual } from './store.ts';

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.webp': 'image/webp',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.ttf': 'font/ttf'
};

function state() {
  return {
    queued: queuedCodes(),
    contracts: readContracts(),
    manual: readManual(),
    loggedIn: hasToken(),
    login: loginState()
  };
}

export type ServerState = ReturnType<typeof state>;

function send(res: http.ServerResponse, status: number, body: unknown) {
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
}

// The { codes } body of a queue or manual-marks update, or null if it isn't a list of known maps.
async function readCodes(req: http.IncomingMessage): Promise<string[] | null> {
  let text = '';
  for await (const chunk of req) text += chunk;
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    return null;
  }
  const codes = typeof body === 'object' && body !== null && 'codes' in body ? body.codes : undefined;
  return Array.isArray(codes) && codes.every((code) => typeof code === 'string' && MAP_BY_CODE.has(code))
    ? codes
    : null;
}

// A built file by its path inside dist, from the exe's embedded assets or the dist folder.
function readAsset(name: string): Buffer | null {
  if (PACKAGED) {
    try {
      return Buffer.from(sea.getAsset(name));
    } catch {
      return null;
    }
  }
  const file = path.join(DIST, name);
  return file.startsWith(DIST + path.sep) && fs.existsSync(file) && fs.statSync(file).isFile()
    ? fs.readFileSync(file)
    : null;
}

function serveStatic(pathname: string, res: http.ServerResponse) {
  let name = path.posix.normalize(decodeURIComponent(pathname)).replace(/^\/+/, '');
  let body = readAsset(name);
  if (!body) {
    name = 'index.html';
    body = readAsset(name);
  }
  if (!body) {
    res.writeHead(404).end('App not built. Run "npm start" (it builds first) or "npm run dev".');
    return;
  }
  res.writeHead(200, { 'Content-Type': TYPES[path.posix.extname(name)] ?? 'application/octet-stream' });
  res.end(body);
}

// Browsers label every request with where it came from. Without this check, any web page you visit
// could post to this port and clear your queue, sign you out, or quit the app.
function isCrossSite(req: http.IncomingMessage) {
  const site = req.headers['sec-fetch-site'];
  if (typeof site === 'string') return site !== 'same-origin' && site !== 'none';
  const origin = req.headers.origin;
  if (origin === undefined) return false;
  try {
    return new URL(origin).host !== req.headers.host;
  } catch {
    return true;
  }
}

const BAD_CODES = { error: 'Expected { codes: string[] } of known map codes.' };

const server = http.createServer(async (req, res) => {
  const { pathname } = new URL(req.url ?? '/', 'http://localhost');
  try {
    if (pathname.startsWith('/api/') && req.method !== 'GET' && isCrossSite(req))
      return send(res, 403, { error: 'Refused: request came from another site.' });

    if (pathname === '/api/state' && req.method === 'GET') return send(res, 200, state());

    if (pathname === '/api/game' && req.method === 'GET') return send(res, 200, gameState());

    if (pathname === '/api/launch' && req.method === 'POST') {
      execFile('cmd', ['/c', 'start', '', `steam://rungameid/${TF2_APPID}`]);
      return send(res, 200, gameState());
    }

    if (pathname === '/api/queue' && req.method === 'PUT') {
      const codes = await readCodes(req);
      if (!codes) return send(res, 400, BAD_CODES);
      writeQueue(codes);
      return send(res, 200, state());
    }

    if (pathname === '/api/manual' && req.method === 'PUT') {
      const codes = await readCodes(req);
      if (!codes) return send(res, 400, BAD_CODES);
      writeManual(codes);
      return send(res, 200, state());
    }

    if (pathname === '/api/login' && req.method === 'POST') {
      await startLogin();
      return send(res, 200, state());
    }

    if (pathname === '/api/login' && req.method === 'DELETE') {
      cancelLogin();
      return send(res, 200, state());
    }

    if (pathname === '/api/logout' && req.method === 'POST') {
      cancelLogin();
      deleteToken();
      return send(res, 200, state());
    }

    if (pathname === '/api/sync' && req.method === 'POST') {
      await runSync();
      game.autoSyncError = null;
      return send(res, 200, state());
    }

    if (pathname === '/api/quit' && req.method === 'POST') {
      send(res, 200, {});
      return shutdown();
    }

    if (pathname.startsWith('/api/')) return send(res, 404, { error: 'Not found' });
    serveStatic(pathname, res);
  } catch (err) {
    send(res, 500, { error: err instanceof Error ? err.message : String(err) });
  }
});

function tryRestore(done: string) {
  try {
    if (restoreBackup()) console.log(done);
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
  }
}

let closing = false;
function shutdown() {
  if (closing) return;
  closing = true;
  tryRestore('Restored your original casual_criteria.vdf.');
  process.exit(0);
}

// SIGHUP is how Windows reports the console window being closed; Node gets a few seconds to clean up.
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGBREAK'] as const) process.on(signal, shutdown);

if (TF2_DIR) console.log(`TF2 at ${TF2_DIR}`);
else console.warn(NO_TF2);

tryRestore('Found a backup from a previous run and restored casual_criteria.vdf.');
watchGame();

server.listen(PORT, '127.0.0.1', () => {
  const url = `http://127.0.0.1:${PORT}`;
  console.log(`Halloween contracts app at ${url}`);
  console.log('Close this window or press Ctrl+C to quit; your original map selection is restored then.');
  if (!process.argv.includes('--no-open')) execFile('cmd', ['/c', 'start', '', url]);
});
