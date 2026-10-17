import { LogIn, Play, RefreshCw, Search, TriangleAlert } from 'lucide-react';
import { useDeferredValue, useEffect, useRef, useState } from 'react';

import { MAP_BY_CODE, MAPS, type HalloweenMap } from '../maps.ts';
import type { GameState, ServerState } from './api.ts';
import { MapTile } from './MapTile.tsx';
import { NowPlaying } from './NowPlaying.tsx';

type Filter = 'all' | 'todo' | 'done' | 'queued';

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

function Progress({ done, total }: { done: number; total: number }) {
  return (
    <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={done}>
      <div className="progress-fill" style={{ width: `${total ? (done / total) * 100 : 0}%` }} />
    </div>
  );
}

type ContractsViewProps = {
  className: string;
  server: ServerState | null;
  game: GameState | null;
  queued: Set<string>;
  completed: Set<string>;
  doneState: (code: string) => 'synced' | 'manual' | null;
  syncedAt: string | null;
  syncing: boolean;
  launching: boolean;
  /** The "Restore original casual map selection on quit" setting; it changes what closing promises. */
  restoreOnQuit: boolean;
  onLaunch: () => void;
  onSync: () => void;
  onSignIn: () => void;
  onSaveQueue: (next: Set<string>) => void;
  onToggleManual: (code: string) => void;
  onClose: () => void;
  onOpenSettings: () => void;
};

// The contracts tool: search, Steam and queue actions, the map tiles by section, and Now playing.
export function ContractsView({
  className,
  server,
  game,
  queued,
  completed,
  doneState,
  syncedAt,
  syncing,
  launching,
  restoreOnQuit,
  onLaunch,
  onSync,
  onSignIn,
  onSaveQueue,
  onToggleManual,
  onClose,
  onOpenSettings
}: ContractsViewProps) {
  // Finished matches you answered "Not yet" for, keyed by map and end time so a rematch asks again.
  const [dismissed, setDismissed] = useState(new Set<string>());
  // Tile to scroll to once the filters are cleared and it has rendered.
  const [focusCode, setFocusCode] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const searchInput = useRef<HTMLInputElement>(null);
  const search = useDeferredValue(query.trim().toLowerCase());

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

  function showTile(code: string) {
    setQuery('');
    setFilter('all');
    setFocusCode(code);
  }

  const lastMatch = game?.played.find(
    (match) => !completed.has(match.code) && !dismissed.has(`${match.code}@${match.at}`)
  );
  const playingMap = game?.map ? MAP_BY_CODE.get(game.map) : undefined;
  // The panel shows the map being played, or else the last finished match still waiting for an answer.
  const panelMap = playingMap ?? (lastMatch && MAP_BY_CODE.get(lastMatch.code));

  return (
    <section id="view-contracts" className={className} role="tabpanel" aria-labelledby="tab-contracts">
      {/* Only the controls stick while you scroll the list. Row 1 acts, row 2 picks what the list shows. */}
      <div className="top">
        <div className="wrap bar">
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
          <div className="cluster">
            {!game?.running && (
              <button type="button" className="btn btn-accent" onClick={onLaunch} disabled={!server || launching}>
                <Play aria-hidden="true" />
                {launching ? 'Starting TF2…' : 'Launch TF2'}
              </button>
            )}
            {server && !server.loggedIn ? (
              <button type="button" className="btn" onClick={onSignIn}>
                <LogIn aria-hidden="true" />
                Sign in with Steam
              </button>
            ) : (
              <button
                type="button"
                className="btn"
                onClick={onSync}
                disabled={syncing || !server || !!game?.running}
                title={
                  game?.running
                    ? "Steam can't be read while TF2 runs. It syncs by itself when TF2 closes."
                    : 'It also syncs by itself when you launch or close TF2.'
                }
              >
                <RefreshCw className={syncing ? 'spin' : undefined} aria-hidden="true" />
                {syncing ? 'Syncing…' : 'Sync'}
              </button>
            )}
            <span className="small">
              {server && !server.loggedIn
                ? 'Sign in once with the Steam mobile app to sync your contracts.'
                : syncedAt
                  ? `Synced ${syncedAt}`
                  : 'Not synced yet'}
            </span>
          </div>
          <div className="cluster">
            <span className="count">
              <strong>{queued.size}</strong> queued
            </span>
            <button
              type="button"
              className="btn"
              disabled={!server}
              onClick={() =>
                onSaveQueue(new Set(MAPS.filter((map) => !completed.has(map.code)).map((map) => map.code)))
              }
            >
              Queue all to do
            </button>
            <button
              type="button"
              className="btn"
              disabled={!server || queued.size === 0}
              onClick={() => onSaveQueue(new Set())}
            >
              Clear
            </button>
          </div>
        </div>

        <div className="wrap bar2">
          <div className="ftabs" role="group" aria-label="Show">
            {FILTERS.map(({ value, label }) => (
              <button
                key={value}
                type="button"
                className="ftab"
                aria-pressed={filter === value}
                onClick={() => setFilter(value)}
              >
                {label} <span>{counts[value]}</span>
              </button>
            ))}
          </div>
          <div className="total">
            <p>
              <strong>{doneCount}</strong> / {MAPS.length} completed
            </p>
            <Progress done={doneCount} total={MAPS.length} />
          </div>
        </div>

        {panelMap && (
          <NowPlaying
            map={panelMap}
            ended={!playingMap}
            done={doneState(panelMap.code)}
            onMark={() => onToggleManual(panelMap.code)}
            onShow={() => showTile(panelMap.code)}
            onDismiss={() => lastMatch && setDismissed(new Set(dismissed).add(`${lastMatch.code}@${lastMatch.at}`))}
          />
        )}
      </div>

      <main className="wrap">
        {game && !game.tf2Found && (
          <div className="banner" role="status">
            <TriangleAlert aria-hidden="true" />
            <p>TF2 was not found in your Steam libraries, so queueing is off.</p>
            <button type="button" className="btn small" onClick={onOpenSettings}>
              Choose folder
            </button>
          </div>
        )}
        {game?.tf2Found && !game.logFound && (
          <div className="banner" role="status">
            <TriangleAlert aria-hidden="true" />
            <p>
              To follow your matches here, add <strong>-condebug</strong> to TF2's launch options in Steam.
            </p>
          </div>
        )}
        <p className="hint">
          Click maps to queue them, then click <strong>Restore</strong> in TF2's Casual map selection.
          {restoreOnQuit && ' Closing this app puts your original selection back.'}
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
                      onToggle={() => onSaveQueue(toggled(queued, map.code))}
                      onMark={() => onToggleManual(map.code)}
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
          Screenshots from the{' '}
          <a href="https://wiki.teamfortress.com/wiki/Halloween_map" target="_blank" rel="noreferrer">
            TF2 Wiki
          </a>
          .
        </p>
        <button type="button" className="link muted" onClick={onClose}>
          {restoreOnQuit ? 'Close app and restore map selection' : 'Close app'}
        </button>
      </footer>
    </section>
  );
}
