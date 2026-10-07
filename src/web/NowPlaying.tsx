import { ClipboardCheck, LocateFixed } from 'lucide-react';

import type { HalloweenMap } from '../maps.ts';

type NowPlayingProps = {
  map: HalloweenMap;
  /** The match is over and the app is asking whether the contract got done. */
  ended: boolean;
  done: 'synced' | 'manual' | null;
  onMark: () => void;
  onShow: () => void;
  onDismiss: () => void;
};

// The map TF2 is on, with where its contract sits in the ConTracker, so it's easy to find in game.
export function NowPlaying({ map, ended, done, onMark, onShow, onDismiss }: NowPlayingProps) {
  return (
    <div className="now">
      <div className="wrap now-row">
        <img className="now-shot" src={map.image} alt="" width={128} height={72} />
        <div className="now-info">
          <p className="kicker">
            {ended ? (
              'Match over'
            ) : (
              <>
                <span className="game-dot" aria-hidden="true" /> Playing now
              </>
            )}
          </p>
          <h2 className="now-name">{map.name}</h2>
          <p className="now-meta">
            ConTracker <strong>{map.contractFolder}</strong>
            <span aria-hidden="true"> · </span>
            {map.mode}
            <span aria-hidden="true"> · </span>
            {done ? (
              <span className="now-done">{done === 'synced' ? 'Contract done' : 'Marked done'}</span>
            ) : ended ? (
              'Did you finish its contract?'
            ) : (
              'Contract not done'
            )}
          </p>
        </div>
        <div className="now-actions">
          {!done && (
            <button type="button" className="btn" onClick={onMark}>
              <ClipboardCheck aria-hidden="true" />
              Mark done
            </button>
          )}
          {ended ? (
            <button type="button" className="btn" onClick={onDismiss}>
              Not yet
            </button>
          ) : (
            <button type="button" className="btn" onClick={onShow}>
              <LocateFixed aria-hidden="true" />
              Find in list
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
