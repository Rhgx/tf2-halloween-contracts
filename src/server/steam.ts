// Everything that talks to Steam: the one-time QR sign-in, and reading completed contracts from
// the TF2 Game Coordinator with the token it produced.

import QRCode from 'qrcode';
import { EAuthTokenPlatformType, LoginSession } from 'steam-session';
import SteamUser from 'steam-user';

import { bigint, decode, messages, num } from '../lib/protobuf.ts';
import { MAPS } from '../maps.ts';
import { TF2_APPID } from './config.ts';
import { hasToken, readManual, readToken, saveToken, writeContracts, writeManual } from './store.ts';

const GC_CLIENT_HELLO = 4006;
const SO_CACHE_SUBSCRIBED = 24;
const SO_CACHE_SUBSCRIPTION_CHECK = 27;
const SO_CACHE_SUBSCRIPTION_REFRESH = 28;
const SO_TYPE_QUEST_MAP_NODE = 44;

// --- Sign-in by QR code --------------------------------------------------------------------------
// Produces a SteamClient refresh token, the kind steam-user logs on with.

type Login = { status: 'waiting' | 'scanned' | 'failed'; qr: string; error?: string };

let login: { session: LoginSession; state: Login } | null = null;

export const loginState = () => login?.state ?? null;

export function cancelLogin() {
  login?.session.cancelLoginAttempt();
  login = null;
}

export async function startLogin() {
  cancelLogin();
  const session = new LoginSession(EAuthTokenPlatformType.SteamClient);
  session.loginTimeout = 120_000;
  const { qrChallengeUrl } = await session.startWithQR();
  if (!qrChallengeUrl) throw new Error('Steam did not return a QR code.');
  const qr = await QRCode.toDataURL(qrChallengeUrl, { margin: 2 });
  const current: { session: LoginSession; state: Login } = { session, state: { status: 'waiting', qr } };
  login = current;

  const fail = (error: string) => {
    if (login === current) current.state = { ...current.state, status: 'failed', error };
  };
  session.on('remoteInteraction', () => (current.state = { ...current.state, status: 'scanned' }));
  session.on('authenticated', () => {
    saveToken(session.refreshToken);
    if (login === current) login = null;
  });
  session.on('timeout', () => fail('The QR code expired. Try again.'));
  session.on('error', (err) => fail(`Steam: ${err.message}`));
}

// --- Contracts from the Game Coordinator -----------------------------------------------------------

// Logs in with the saved refresh token, says hello to the TF2 GC and reads the account's
// CSOQuestMapNode objects from its shared object cache. Returns node defindexes whose primary
// star (contract completed) is earned.
async function fetchCompletedNodes() {
  if (!hasToken()) throw new Error('Not signed in to Steam. Use "Sign in with Steam" first.');
  const refreshToken = readToken();
  const user = new SteamUser({ autoRelogin: false });

  try {
    return await new Promise<Set<number>>((resolve, reject) => {
      let hello: NodeJS.Timeout | undefined;
      const timeout = setTimeout(
        () => reject(new Error('Steam or the TF2 Game Coordinator did not respond in time.')),
        45_000
      );
      const done = () => {
        clearTimeout(timeout);
        clearInterval(hello);
      };

      user.on('error', (err) => {
        done();
        reject(new Error(`Steam: ${err.message}`));
      });

      // Steam reports whether another session (e.g. TF2 running on this PC) is in a game. Never
      // take that session over; reading contracts would kick you out of TF2.
      user.once('playingState', (blocked, playingApp) => {
        if (blocked) {
          done();
          reject(new Error(`Your account is in a game elsewhere (app ${playingApp}). Close TF2, then sync again.`));
          return;
        }
        user.gamesPlayed([TF2_APPID]);
        const sayHello = () => user.sendToGC(TF2_APPID, GC_CLIENT_HELLO, {}, Buffer.alloc(0));
        sayHello();
        hello = setInterval(sayHello, 5000);
      });

      user.on('receivedFromGC', (appid, msgType, payload) => {
        if (appid !== TF2_APPID) return;
        // The GC first asks whether we already hold a cached copy (CMsgSOCacheSubscriptionCheck
        // { owner = 1 (fixed64), version = 2 }). Asking for a refresh with the same owner makes it
        // send the full cache.
        if (msgType === SO_CACHE_SUBSCRIPTION_CHECK) {
          const owner = bigint(decode(payload), 1);
          if (owner === undefined) return;
          const refresh = Buffer.alloc(9); // CMsgSOCacheSubscriptionRefresh { owner = 1 (fixed64) }
          refresh[0] = (1 << 3) | 1;
          refresh.writeBigUInt64LE(owner, 1);
          user.sendToGC(TF2_APPID, SO_CACHE_SUBSCRIPTION_REFRESH, {}, refresh);
          return;
        }
        if (msgType !== SO_CACHE_SUBSCRIBED) return;
        // CMsgSOCacheSubscribed { owner = 1 (fixed64) | owner_soid = 4 { id = 2 }, objects = 2 { type_id = 1, object_data = 2 } }
        const cache = decode(payload);
        const owner = bigint(cache, 1) ?? bigint(messages(cache, 4)[0] ?? new Map(), 2);
        if (owner !== undefined && owner !== BigInt(user.steamID?.getSteamID64() ?? 0)) return;
        // CSOQuestMapNode { defindex = 3, star_0_earned = 6 }
        const nodes = messages(cache, 2)
          .filter((type) => num(type, 1) === SO_TYPE_QUEST_MAP_NODE)
          .flatMap((type) => messages(type, 2));
        done();
        resolve(new Set(nodes.filter((node) => num(node, 6) === 1).map((node) => num(node, 3) ?? -1)));
      });

      user.logOn({ refreshToken });
    });
  } finally {
    user.logOff();
  }
}

async function syncContracts() {
  const completedNodes = await fetchCompletedNodes();
  const completed = MAPS.filter((map) => completedNodes.has(map.contractNode)).map((map) => map.code);
  writeContracts({ completed, syncedAt: new Date().toISOString() });
  // Manual marks are only a stand-in until the GC knows; drop the ones it now confirms.
  writeManual(readManual().filter((code) => !completed.includes(code)));
}

let syncing: Promise<void> | null = null;

export const isSyncing = () => syncing !== null;

// One GC session at a time; a second request waits for the first.
export function runSync() {
  return (syncing ??= syncContracts().finally(() => (syncing = null)));
}
