import { Check, ClipboardCheck, ExternalLink, Undo2 } from 'lucide-react';

import type { HalloweenMap } from '../maps.ts';

type MapTileProps = {
  map: HalloweenMap;
  /** Where the completion comes from: a Steam sync, your own mark, or not completed. */
  done: 'synced' | 'manual' | null;
  queued: boolean;
  /** TF2 is in a match on this map right now. */
  playing: boolean;
  onToggle: () => void;
  onMark: () => void;
};

// The tile toggles whether you queue for the map; the labeled checkbox mirrors the casual menu's.
// Completion is a text label on the screenshot, so it never reads as a second checkbox. The footer
// marks a contract done by hand, or says Steam confirmed it.
export function MapTile({ map, done, queued, playing, onToggle, onMark }: MapTileProps) {
  return (
    <li
      id={`map-${map.code}`}
      className={`map${done ? ' is-done' : ''}${queued ? ' is-queued' : ''}${playing ? ' is-playing' : ''}`}
    >
      <button
        type="button"
        className="map-toggle"
        aria-pressed={queued}
        onClick={onToggle}
        title={`${map.code}\n${queued ? 'Queued. Click to remove it from your casual queue.' : 'Click to queue for this map.'}`}
      >
        <span className="map-shot">
          <img src={map.image} alt="" loading="lazy" decoding="async" />
          <span className="map-check" aria-hidden="true">
            <span className="map-box">{queued && <Check strokeWidth={3} />}</span>
            <span className="map-label">{queued ? 'Queued' : 'Queue'}</span>
          </span>
          {done && (
            <span className={`map-status${done === 'manual' ? ' is-manual' : ''}`}>
              {done === 'synced' ? 'Contract done' : 'Marked done'}
            </span>
          )}
          {playing && <span className="map-playing">Playing now</span>}
        </span>
        <span className="map-info">
          <span className="map-name">{map.name}</span>
          <span className="map-meta">{map.mode}</span>
          <span className="map-meta">ConTracker: {map.contractFolder}</span>
        </span>
      </button>
      <a
        className="map-wiki"
        href={map.wikiUrl}
        target="_blank"
        rel="noreferrer"
        aria-label={`${map.name} on the TF2 Wiki`}
        title="Open on the TF2 Wiki"
      >
        <ExternalLink />
      </a>
      <div className="map-foot">
        {done === 'synced' ? (
          <span className="map-confirmed">
            <Check aria-hidden="true" />
            Confirmed by Steam
          </span>
        ) : (
          <button
            type="button"
            className="map-mark"
            onClick={onMark}
            aria-label={done === 'manual' ? `Unmark the ${map.name} contract` : `Mark the ${map.name} contract done`}
            title={
              done === 'manual'
                ? 'Undo your mark. Use this if you marked it by mistake.'
                : 'Mark it done by hand, e.g. while TF2 is running. The next sync confirms it.'
            }
          >
            {done === 'manual' ? <Undo2 aria-hidden="true" /> : <ClipboardCheck aria-hidden="true" />}
            {done === 'manual' ? 'Unmark' : 'Mark done'}
          </button>
        )}
      </div>
    </li>
  );
}
