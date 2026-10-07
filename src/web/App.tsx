import { LogIn, Play, RefreshCw, Search, X } from 'lucide-react';
import { useDeferredValue, useEffect, useRef, useState } from 'react';

import { MAP_BY_CODE, MAPS, type HalloweenMap } from '../maps.ts';
import { api, errorMessage, type GameState, type ServerState } from './api.ts';
import { MapTile } from './MapTile.tsx';
import { NowPlaying } from './NowPlaying.tsx';

type Filter = 'all' | 'todo' | 'done' | 'queued';
type Toast = { message: string; kind: 'ok' | 'error' };

const FILTERS: { value: Filter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'todo', label: 'To do' },
  { value: 'done', label: 'Completed' },
  { value: 'queued', label: 'Queued' }
];

// Maps grouped by casual menu section, in menu order.
const SECTIONS = [...Map.groupBy(MAPS, (map) => map.section).values()].map((maps) => ({
  tab: maps[0].tab,
  name: maps[0].section,
  maps
}));

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

function Progress({ done, total }: { done: number; total: number }) {
  return (
    <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={done}>
      <div className="progress-fill" style={{ width: `${total ? (done / total) * 100 : 0}%` }} />
    </div>
  );
}

export function App() {
  const [server, setServer] = useState<ServerState | null>(null);
  const [queued, setQueued] = useState(new Set<string>());
  const [manual, setManual] = useState(new Set<string>());
  const [syncing, setSyncing] = useState(false);
  const [game, setGame] = useState<GameState | null>(null);
  const [launching, setLaunching] = useState(false);
  // Finished matches you answered "Not yet" for, keyed by map and end time so a rematch asks again.
  const [dismissed, setDismissed] = useState(new Set<string>());
  // Tile to scroll to once the filters are cleared and it has rendered.
  const [focusCode, setFocusCode] = useState<string | null>(null);
  const [closed, setClosed] = useState(false);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [toast, setToast] = useState<Toast | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  // Writes run one after another so a slow request can't land after a newer one.
  const writes = useRef(Promise.resolve());
  const search = useDeferredValue(query.trim().toLowerCase());

  const synced = new Set(server?.contracts?.completed ?? []);
  // Marked by hand counts too, until a sync confirms it (the server then drops the mark).
  const completed = new Set([...synced, ...manual]);

  function applyState(state: ServerState) {
    setServer(state);
    setQueued(new Set(state.queued));
    setManual(new Set(state.manual));
  }

  const showError = (err: unknown) => setToast({ message: errorMessage(err), kind: 'error' });

  useEffect(() => {
    api('/api/state').then(applyState, showError);
  }, []);

  useEffect(() => {
    const poll = () => api<GameState>('/api/game').then(setGame, () => setGame(null));
    poll();
    const timer = setInterval(poll, 2000);
    return () => clearInterval(timer);
  }, []);

  // The server syncs by itself once TF2 closes; pick up the result when it lands.
  const liveSyncedAt = game?.syncedAt;
  useEffect(() => {
    if (!liveSyncedAt || !server || liveSyncedAt === server.contracts?.syncedAt) return;
    writes.current = writes.current
      .then(() => api('/api/state'))
      .then((state) => {
        applyState(state);
        setToast({ message: syncedMessage(state), kind: 'ok' });
      }, showError);
  }, [liveSyncedAt]);

  const autoSyncError = game?.autoSyncError;
  useEffect(() => {
    if (autoSyncError) setToast({ message: `Couldn't sync after TF2 closed: ${autoSyncError}`, kind: 'error' });
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
        const state = await api('/api/state');
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

  // Search is deferred, so the tile can show up a render after the filters clear.
  useEffect(() => {
    const tile = focusCode && document.getElementById(`map-${focusCode}`);
    if (!tile) return;
    tile.scrollIntoView({ behavior: 'smooth', block: 'center' });
    tile.querySelector('button')?.focus({ preventScroll: true });
    // A fading outline, since a mouse click doesn't show the focus ring.
    tile.animate({ outlineColor: ['var(--text)', 'transparent'] }, 1500);
    setFocusCode(null);
  }, [focusCode, search, filter]);

  // "/" jumps to search, like most list UIs.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '/' || e.target instanceof HTMLInputElement) return;
      e.preventDefault();
      searchInput.current?.focus();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const doneCount = MAPS.filter((map) => completed.has(map.code)).length;
  const counts: Record<Filter, number> = {
    all: MAPS.length,
    todo: MAPS.length - doneCount,
    done: doneCount,
    queued: queued.size
  };

  const matchesFilter = (map: HalloweenMap) =>
    filter === 'all' || (filter === 'queued' ? queued.has(map.code) : completed.has(map.code) === (filter === 'done'));

  const isShown = (map: HalloweenMap) =>
    matchesFilter(map) &&
    (!search ||
      map.name.toLowerCase().includes(search) ||
      map.code.includes(search) ||
      map.contractFolder.includes(search) ||
      map.mode.toLowerCase().includes(search));

  const visibleSections = SECTIONS.map((section) => ({ ...section, shown: section.maps.filter(isShown) })).filter(
    (section) => section.shown.length > 0
  );

  function write(path: '/api/queue' | '/api/manual', codes: string[], what: string) {
    writes.current = writes.current
      .then(() => api(path, { method: 'PUT', body: JSON.stringify({ codes }) }))
      .then(setServer, (err: unknown) => {
        setToast({ message: `Couldn't save ${what}: ${errorMessage(err)}`, kind: 'error' });
        return api('/api/state').then(applyState, () => {});
      });
  }

  function saveQueue(next: Set<string>) {
    setQueued(next);
    write('/api/queue', [...next], 'your casual queue');
  }

  function toggleManual(code: string) {
    const next = toggled(manual, code);
    setManual(next);
    write('/api/manual', [...next], 'your manual marks');
    // A finished contract needs no more matches on that map.
    if (next.has(code) && queued.has(code)) saveQueue(toggled(queued, code));
  }

  async function account(path: string, method: 'POST' | 'DELETE') {
    try {
      setServer(await api(path, { method }));
    } catch (err) {
      showError(err);
    }
  }

  async function sync() {
    setSyncing(true);
    try {
      const state = await api('/api/sync', { method: 'POST' });
      setServer(state);
      setToast({ message: syncedMessage(state), kind: 'ok' });
    } catch (err) {
      showError(err);
    } finally {
      setSyncing(false);
    }
  }

  function showTile(code: string) {
    setQuery('');
    setFilter('all');
    setFocusCode(code);
  }

  // Syncs first so the session starts from Steam's latest, since syncing is blocked while TF2 runs.
  async function launch() {
    setLaunching(true);
    if (server?.loggedIn) await sync();
    try {
      await api<GameState>('/api/launch', { method: 'POST' });
    } catch (err) {
      setLaunching(false);
      showError(err);
    }
  }

  async function closeApp() {
    await writes.current;
    await fetch('/api/quit', { method: 'POST' }).catch(() => {});
    setClosed(true);
  }

  if (closed) {
    return (
      <main className="wrap">
        <p className="empty">Closed. Your original casual map selection is back in place. You can close this tab.</p>
      </main>
    );
  }

  const syncedAt = server?.contracts
    ? new Date(server.contracts.syncedAt).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
    : null;
  const isSyncing = syncing || !!game?.syncing;
  const lastMatch = game?.played.find(
    (match) => !completed.has(match.code) && !dismissed.has(`${match.code}@${match.at}`)
  );
  const playingMap = game?.map ? MAP_BY_CODE.get(game.map) : undefined;
  // The panel shows the map being played, or else the last finished match still waiting for an answer.
  const panelMap = playingMap ?? (lastMatch && MAP_BY_CODE.get(lastMatch.code));
  const doneState = (code: string) => (synced.has(code) ? 'synced' : manual.has(code) ? 'manual' : null);

  return (
    <>
      <header className="wrap masthead">
        <div>
          <p className="kicker">Team Fortress 2</p>
          <h1>Halloween Contracts</h1>
        </div>
        <div className="total">
          <p>
            <strong>{doneCount}</strong> / {MAPS.length} completed
          </p>
          <Progress done={doneCount} total={MAPS.length} />
        </div>
      </header>

      {/* Only the controls stick while you scroll the list; the title above doesn't need to. */}
      <div className="top">
        <div className="wrap toolbar">
          <label className="search">
            <Search aria-hidden="true" />
            <input
              ref={searchInput}
              type="search"
              placeholder="Search maps, modes, file names or ConTracker folders"
              aria-label="Search maps"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </label>
          <div className="segmented" role="group" aria-label="Show">
            {FILTERS.map(({ value, label }) => (
              <button key={value} type="button" aria-pressed={filter === value} onClick={() => setFilter(value)}>
                {label} <span>{counts[value]}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="wrap queue-bar">
          {game?.running ? (
            <span className="game-status" role="status">
              <span className="game-dot" aria-hidden="true" />
              {gameStatus(game)}
            </span>
          ) : (
            <button type="button" className="btn btn-accent" onClick={launch} disabled={!server || launching}>
              <Play aria-hidden="true" />
              {launching ? 'Starting TF2…' : 'Launch TF2'}
            </button>
          )}
          {server && !server.loggedIn ? (
            <button type="button" className="btn" onClick={() => account('/api/login', 'POST')}>
              <LogIn aria-hidden="true" />
              Sign in with Steam
            </button>
          ) : (
            <button
              type="button"
              className="btn"
              onClick={sync}
              disabled={isSyncing || !server || !!game?.running}
              title={
                game?.running
                  ? "Steam can't be read while TF2 runs. It syncs by itself when TF2 closes."
                  : 'It also syncs by itself when you launch or close TF2.'
              }
            >
              <RefreshCw className={isSyncing ? 'spin' : undefined} aria-hidden="true" />
              {isSyncing ? 'Syncing…' : 'Sync'}
            </button>
          )}
          <span className="queue-note">
            {server && !server.loggedIn
              ? 'Sign in once with the Steam mobile app to sync your contracts.'
              : syncedAt
                ? `Synced ${syncedAt}`
                : 'Not synced yet'}
          </span>
          <span className="queue-actions">
            <strong>{queued.size}</strong> queued
            <button
              type="button"
              className="btn"
              disabled={!server}
              onClick={() => saveQueue(new Set(MAPS.filter((map) => !completed.has(map.code)).map((map) => map.code)))}
            >
              Queue all to do
            </button>
            <button
              type="button"
              className="btn"
              disabled={!server || queued.size === 0}
              onClick={() => saveQueue(new Set())}
            >
              Clear
            </button>
          </span>
        </div>

        {panelMap && (
          <NowPlaying
            map={panelMap}
            ended={!playingMap}
            done={doneState(panelMap.code)}
            onMark={() => toggleManual(panelMap.code)}
            onShow={() => showTile(panelMap.code)}
            onDismiss={() => lastMatch && setDismissed(new Set(dismissed).add(`${lastMatch.code}@${lastMatch.at}`))}
          />
        )}
      </div>

      <main className="wrap">
        <p className="hint">
          Click maps to queue them, then click <strong>Restore</strong> in TF2's Casual map selection. Closing this app
          puts your original selection back.
          {game && !game.tf2Found ? (
            <>
              {' '}
              TF2 was not found in your Steam libraries, so queueing is off. Set <strong>TF2_DIR</strong> to its folder
              and restart the app.
            </>
          ) : (
            game &&
            !game.logFound && (
              <>
                {' '}
                To follow your matches here, add <strong>-condebug</strong> to TF2's launch options in Steam.
              </>
            )
          )}
        </p>
        {visibleSections.length === 0 ? (
          <p className="empty">
            No maps match.{' '}
            <button
              type="button"
              className="link"
              onClick={() => {
                setQuery('');
                setFilter('all');
              }}
            >
              Show all maps
            </button>
          </p>
        ) : (
          visibleSections.map((section, i) => {
            const sectionDone = section.maps.filter((map) => completed.has(map.code)).length;
            return (
              <section key={section.name} className="section" aria-labelledby={`section-${i}`}>
                <div className="section-head">
                  <div>
                    <p className="kicker">{section.tab} tab</p>
                    <h2 id={`section-${i}`}>{section.name}</h2>
                  </div>
                  <div className="section-count">
                    <span>
                      {sectionDone} / {section.maps.length}
                    </span>
                    <Progress done={sectionDone} total={section.maps.length} />
                  </div>
                </div>
                <ul className="grid">
                  {section.shown.map((map) => (
                    <MapTile
                      key={map.code}
                      map={map}
                      done={doneState(map.code)}
                      queued={queued.has(map.code)}
                      playing={game?.map === map.code}
                      onToggle={() => saveQueue(toggled(queued, map.code))}
                      onMark={() => toggleManual(map.code)}
                    />
                  ))}
                </ul>
              </section>
            );
          })
        )}
      </main>

      <footer className="wrap footer">
        <p>
          Map list from TF2's casual menu, contracts from your ConTracker. Screenshots from the{' '}
          <a href="https://wiki.teamfortress.com/wiki/Halloween_map" target="_blank" rel="noreferrer">
            TF2 Wiki
          </a>
          .
        </p>
        <div className="footer-actions">
          {server?.loggedIn && (
            <button type="button" className="link muted" onClick={() => account('/api/logout', 'POST')}>
              Sign out of Steam
            </button>
          )}
          <button type="button" className="link muted" onClick={closeApp}>
            Close app and restore map selection
          </button>
        </div>
      </footer>

      {server?.login && (
        <dialog
          className="dialog login"
          aria-labelledby="loginTitle"
          ref={(el) => {
            if (el && !el.open) el.showModal();
          }}
          onCancel={() => account('/api/login', 'DELETE')}
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
            <button type="button" className="btn" onClick={() => account('/api/login', 'DELETE')}>
              Cancel
            </button>
            {server.login.status === 'failed' && (
              <button type="button" className="btn" onClick={() => account('/api/login', 'POST')}>
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
