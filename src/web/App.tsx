import { X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { MAP_BY_CODE, MAPS } from '../maps.ts';
import {
  captureRadarKey,
  chooseTf2Folder,
  errorMessage,
  getGame,
  getRadar,
  getSettings,
  getState,
  launchTf2,
  moveRadar,
  openDataFolder,
  quit,
  resetRadarKey,
  resetRadarLayout,
  setManual as saveManual,
  setQueue,
  setRadarEnabled,
  setRadarSettings,
  setSettings as saveSettings,
  startLogin,
  cancelLogin,
  logout,
  syncContracts,
  useDetectedTf2Folder,
  type GameState,
  type Radar,
  type RadarSettingsPatch,
  type ServerState,
  type Settings,
  type SettingsPatch
} from './api.ts';
import { ContractsView } from './ContractsView.tsx';
import { RadarView } from './RadarView.tsx';
import { SettingsView } from './SettingsView.tsx';
import { TabNav, type TabStatus, type Tool, type ViewId } from './TabNav.tsx';

type Toast = { message: string; kind: 'ok' | 'error' };

function toggled(set: Set<string>, code: string) {
  const next = new Set(set);
  if (!next.delete(code)) next.add(code);
  return next;
}

const syncedMessage = (state: ServerState) => `Synced: ${state.contracts?.completed.length ?? 0} contracts completed.`;

function gameStatus(game: GameState) {
  // Halloween maps get the Now playing panel; anything else is named here.
  if (game.map) return MAP_BY_CODE.has(game.map) ? 'In a match' : `Playing ${game.map}`;
  if (game.inMatch) return 'Joining a match';
  if (game.queued) return 'In queue';
  return 'TF2 is running';
}

// The one line under the Contracts tab: what needs you first, else where TF2 is, else the count.
function contractsStatus(server: ServerState | null, game: GameState | null, done: number): TabStatus {
  const count = `${done} / ${MAPS.length}`;
  if (game && !game.tf2Found) return { text: 'TF2 not found', tone: 'warn' };
  if (game?.running) return { text: `${count} · ${gameStatus(game)}`, tone: 'live' };
  if (server && !server.loggedIn) return { text: 'Not signed in', tone: 'warn' };
  return { text: count };
}

// The line under the Gargoyle radar tab: off, what needs you, else how far the link (and the gargoyle) is.
function radarStatus({ settings, setupDone, status }: Radar): TabStatus {
  if (!settings.enabled) return { text: 'Off' };
  if (!setupDone) return { text: 'Setup needed', tone: 'warn' };
  switch (status.link) {
    case 'unreachable':
      return { text: 'Unreachable', tone: 'warn' };
    case 'wrong-password':
      return { text: 'Wrong password', tone: 'warn' };
    case 'connected':
      return { text: status.reading ? `Gargoyle ${Math.round(status.reading.distM)} m` : 'Connected', tone: 'live' };
    default:
      return { text: 'Waiting for TF2' };
  }
}

export function App() {
  const [server, setServer] = useState<ServerState | null>(null);
  const [queued, setQueued] = useState(new Set<string>());
  const [manual, setManual] = useState(new Set<string>());
  const [syncing, setSyncing] = useState(false);
  const [game, setGame] = useState<GameState | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [radar, setRadar] = useState<Radar | null>(null);
  // The backend answers null where there is no radar (Linux); then there is nothing to poll.
  const [noRadar, setNoRadar] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [closed, setClosed] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [view, setView] = useState<ViewId>('contracts');
  // The view on its way out; the incoming one waits for it so they never stack.
  const [leaving, setLeaving] = useState<ViewId | null>(null);
  // Writes run one after another so a slow request can't land after a newer one.
  const writes = useRef(Promise.resolve());

  const synced = new Set(server?.contracts?.completed ?? []);
  // Marked by hand counts too, until a sync confirms it (the server then drops the mark).
  const completed = new Set([...synced, ...manual]);

  function applyState(state: ServerState) {
    setServer(state);
    setQueued(new Set(state.queued));
    setManual(new Set(state.manual));
  }

  const showError = (err: unknown) => setToast({ message: errorMessage(err), kind: 'error' });
  const loadSettings = () => getSettings().then(setSettings, showError);

  useEffect(() => {
    getState().then(applyState, showError);
    loadSettings();
  }, []);

  useEffect(() => {
    const poll = () => getGame().then(setGame, () => setGame(null));
    poll();
    const timer = setInterval(poll, 2000);
    return () => clearInterval(timer);
  }, []);

  // The radar feeds its tab's status line, so it polls all the time; faster while its view is open.
  const radarOpen = view === 'radar';
  useEffect(() => {
    if (noRadar) return;
    const poll = () =>
      getRadar().then(
        (next) => (next ? setRadar(next) : setNoRadar(true)),
        () => {}
      );
    poll();
    const timer = setInterval(poll, radarOpen ? 500 : 2000);
    return () => clearInterval(timer);
  }, [radarOpen, noRadar]);

  // The server syncs by itself once TF2 closes; pick up the result when it lands.
  const liveSyncedAt = game?.syncedAt;
  useEffect(() => {
    if (!liveSyncedAt || !server || liveSyncedAt === server.contracts?.syncedAt) return;
    writes.current = writes.current
      .then(() => getState())
      .then((state) => {
        applyState(state);
        setToast({ message: syncedMessage(state), kind: 'ok' });
      }, showError);
  }, [liveSyncedAt]);

  const autoSyncError = game?.autoSyncError;
  useEffect(() => {
    if (!autoSyncError) return;
    setToast({ message: `Couldn't sync after TF2 closed: ${autoSyncError}`, kind: 'error' });
    getState().then(applyState, () => {});
  }, [autoSyncError]);

  // "Starting TF2" holds until the game shows up, or gives up after 90 seconds.
  const running = game?.running;
  useEffect(() => {
    if (!launching) return;
    if (running) {
      setLaunching(false);
      return;
    }
    const timer = setTimeout(() => setLaunching(false), 90_000);
    return () => clearTimeout(timer);
  }, [launching, running]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), toast.kind === 'error' ? 8000 : 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  // While a QR sign-in is pending, poll until the phone approves it (login clears) or it fails.
  const loginStatus = server?.login?.status;
  useEffect(() => {
    if (!loginStatus || loginStatus === 'failed') return;
    const timer = setInterval(async () => {
      try {
        const state = await getState();
        setServer(state);
        if (!state.login && state.loggedIn) {
          setToast({ message: 'Signed in to Steam.', kind: 'ok' });
          sync();
        }
      } catch (err) {
        showError(err);
      }
    }, 2000);
    return () => clearInterval(timer);
  }, [loginStatus]);

  // View switch: the outgoing view fades for 80 ms, then the incoming one rises in its place.
  useEffect(() => {
    if (!leaving) return;
    const timer = setTimeout(() => {
      setLeaving(null);
      scrollTo(0, 0);
    }, 80);
    return () => clearTimeout(timer);
  }, [leaving]);

  function openView(next: ViewId) {
    if (next === view) return;
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) scrollTo(0, 0);
    else setLeaving(view);
    setView(next);
  }

  const viewClass = (id: ViewId) =>
    id === leaving ? 'view is-leaving' : id === view && !leaving ? 'view is-active' : 'view';

  function write(save: (codes: string[]) => Promise<ServerState>, codes: string[], what: string) {
    writes.current = writes.current
      .then(() => save(codes))
      .then(setServer, (err: unknown) => {
        setToast({ message: `Couldn't save ${what}: ${errorMessage(err)}`, kind: 'error' });
        return getState().then(applyState, () => {});
      });
  }

  function saveQueue(next: Set<string>) {
    setQueued(next);
    write(setQueue, [...next], 'your casual queue');
  }

  function toggleManual(code: string) {
    const next = toggled(manual, code);
    setManual(next);
    write(saveManual, [...next], 'your manual marks');
    // A finished contract needs no more matches on that map.
    if (next.has(code) && queued.has(code)) saveQueue(toggled(queued, code));
  }

  async function account(action: () => Promise<ServerState>) {
    try {
      setServer(await action());
    } catch (err) {
      showError(err);
    }
  }

  async function sync() {
    setSyncing(true);
    try {
      const state = await syncContracts();
      setServer(state);
      setToast({ message: syncedMessage(state), kind: 'ok' });
    } catch (err) {
      showError(err);
      // The server drops a token Steam revoked; refresh so the sign-in button comes back.
      getState().then(applyState, () => {});
    } finally {
      setSyncing(false);
    }
  }

  // Syncs first so the session starts from Steam's latest, since syncing is blocked while TF2 runs.
  async function launch() {
    setLaunching(true);
    if (server?.loggedIn) await sync();
    try {
      await launchTf2();
    } catch (err) {
      setLaunching(false);
      showError(err);
    }
  }

  async function closeApp() {
    await writes.current;
    try {
      await quit();
      setClosed(true);
    } catch (err) {
      showError(err);
    }
  }

  // Settings apply as you change them: shown at once, put back if the save fails.
  function changeSettings(patch: SettingsPatch) {
    if (!settings) return;
    const previous = settings;
    setSettings({ ...settings, ...patch });
    saveSettings(patch).then(setSettings, (err: unknown) => {
      setSettings(previous);
      setToast({ message: `Couldn't save settings: ${errorMessage(err)}`, kind: 'error' });
    });
  }

  const applySettings = (task: Promise<Settings>) => task.then(setSettings, showError);

  // Radar settings work the same way; a failed save puts the settings back and keeps the latest status.
  function writeRadar(patch: Partial<Radar['settings']>, save: () => Promise<Radar>, what: string) {
    if (!radar) return;
    const previous = radar.settings;
    setRadar({ ...radar, settings: { ...previous, ...patch } });
    save().then(setRadar, (err: unknown) => {
      setRadar((current) => current && { ...current, settings: previous });
      setToast({ message: `Couldn't ${what}: ${errorMessage(err)}`, kind: 'error' });
    });
  }

  const applyRadar = (task: Promise<Radar>) => task.then(setRadar, showError);

  const restoreOnQuit = settings?.restoreQueueOnQuit ?? true;

  if (closed) {
    return (
      <main className="wrap">
        <p className="empty">
          {restoreOnQuit ? 'Closed. Your original casual map selection is back in place.' : 'Closed.'}
        </p>
      </main>
    );
  }

  const syncedAt = server?.contracts
    ? new Date(server.contracts.syncedAt).toLocaleString('en-GB', { dateStyle: 'medium', timeStyle: 'short' })
    : null;
  const doneCount = MAPS.filter((map) => completed.has(map.code)).length;
  const doneState = (code: string) => (synced.has(code) ? 'synced' : manual.has(code) ? 'manual' : null);

  // The tools in the tab strip, in order. The Gargoyle radar is there where the backend has one (Windows);
  // Settings is not a tool and always sits at the right.
  const tools: Tool[] = [{ id: 'contracts', label: 'Contracts', status: contractsStatus(server, game, doneCount) }];
  if (radar) tools.push({ id: 'radar', label: 'Gargoyle radar', status: radarStatus(radar) });

  return (
    <>
      <header className="shell-head">
        <div className="wrap shell-row">
          <TabNav tools={tools} active={view} onSelect={openView} />
        </div>
      </header>

      <ContractsView
        className={viewClass('contracts')}
        server={server}
        game={game}
        queued={queued}
        completed={completed}
        doneState={doneState}
        syncedAt={syncedAt}
        syncing={syncing || !!game?.syncing}
        launching={launching}
        restoreOnQuit={restoreOnQuit}
        onLaunch={launch}
        onSync={sync}
        onSignIn={() => account(startLogin)}
        onSaveQueue={saveQueue}
        onToggleManual={toggleManual}
        onClose={closeApp}
        onOpenSettings={() => openView('settings')}
      />

      {radar && (
        <RadarView
          className={viewClass('radar')}
          radar={radar}
          game={game}
          onEnable={(enabled) =>
            writeRadar({ enabled }, () => setRadarEnabled(enabled), `turn the radar ${enabled ? 'on' : 'off'}`)
          }
          onChange={(patch: RadarSettingsPatch) =>
            writeRadar(patch, () => setRadarSettings(patch), 'save the radar settings')
          }
          onMove={() => applyRadar(moveRadar())}
          onReset={() => applyRadar(resetRadarLayout())}
          onOpenSettings={() => openView('settings')}
        />
      )}

      <SettingsView
        className={viewClass('settings')}
        settings={settings}
        server={server}
        game={game}
        syncedAt={syncedAt}
        onChange={changeSettings}
        onChooseFolder={() => applySettings(chooseTf2Folder())}
        onUseDetected={() => applySettings(useDetectedTf2Folder())}
        onOpenData={() => openDataFolder().catch(showError)}
        onSignIn={() => account(startLogin)}
        onSignOut={() => account(logout)}
        onRetry={loadSettings}
        radar={radar}
        onCaptureKey={(which) => applyRadar(captureRadarKey(which))}
        onResetKey={(which) => applyRadar(resetRadarKey(which))}
      />

      {server?.login && (
        <dialog
          className="dialog login"
          aria-labelledby="loginTitle"
          ref={(el) => {
            if (el && !el.open) el.showModal();
          }}
          onCancel={() => account(cancelLogin)}
        >
          <h2 id="loginTitle">Sign in with Steam</h2>
          <p>
            In the Steam mobile app, open the Steam Guard tab and scan this code, then approve the sign-in. The app
            keeps a login token on this PC so you only scan once; Sign out deletes it.
          </p>
          <img src={server.login.qr} alt="Steam sign-in QR code" width={220} height={220} />
          <p className={server.login.status === 'failed' ? 'login-error' : undefined}>
            {server.login.status === 'failed'
              ? server.login.error
              : server.login.status === 'scanned'
                ? 'Scanned. Approve the sign-in on your phone.'
                : 'Waiting for you to scan the code.'}
          </p>
          <div className="dialog-actions">
            <button type="button" className="btn" onClick={() => account(cancelLogin)}>
              Cancel
            </button>
            {server.login.status === 'failed' && (
              <button type="button" className="btn" onClick={() => account(startLogin)}>
                New code
              </button>
            )}
          </div>
        </dialog>
      )}

      {toast && (
        <div className={`toast toast-${toast.kind}`} role={toast.kind === 'error' ? 'alert' : 'status'}>
          {toast.message}
          <button type="button" aria-label="Dismiss" onClick={() => setToast(null)}>
            <X />
          </button>
        </div>
      )}
    </>
  );
}
