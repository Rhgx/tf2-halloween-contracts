import { Check, Copy, TriangleAlert } from 'lucide-react';
import { useEffect, useState, type ReactNode } from 'react';

import type { GameState, Radar, RadarReading, RadarSettingsPatch } from './api.ts';
import { Field, Group, Switch } from './fields.tsx';

type RadarViewProps = {
  className: string;
  radar: Radar;
  game: GameState | null;
  onEnable: (enabled: boolean) => void;
  onChange: (patch: RadarSettingsPatch) => void;
  onMove: () => void;
  onReset: () => void;
  onOpenSettings: () => void;
};

// Shown in the preview until the radar has a live reading.
const SAMPLE: RadarReading = { angleDeg: 35, distM: 23, heightM: 4.6 };

// The in-game panel's geometry, in 1080p panel units (src-tauri/src/radar/render.rs).
const RING = { cx: 56, cy: 50, rx: 40, ry: 18 };
const FLAT = RING.ry / RING.rx;
const CREAM = '#ebe3cb';
const TAN_DARK = '#756b5e';
const TAN_TEXT = '#9e9380';
const ORANGE = '#e58626';

const metres = (m: number) => (m < 10 ? m.toFixed(1) : m.toFixed(0));

// SVG replica of the overlay: compass in perspective, the arrow on its floor, the height pin, the two texts.
function Panel({ reading }: { reading: RadarReading }) {
  const arrived = reading.distM < 2.5;
  const level = Math.abs(reading.heightM) < 1.5;
  const accent = arrived ? ORANGE : CREAM;
  const rad = (reading.angleDeg * Math.PI) / 180;
  const [sin, cos] = [Math.sin(rad), Math.cos(rad)];
  // The arrow lies on the ground plane: forward is up the screen, foreshortened by the ring's flatness.
  const ground = (side: number, fwd: number) => ({ x: RING.cx + side, y: RING.cy - fwd * FLAT });
  const tip = arrived ? { x: RING.cx, y: RING.cy } : ground(sin * 0.85 * RING.rx, cos * 0.85 * RING.rx);
  const base = ground(-sin * 0.35 * RING.rx, -cos * 0.35 * RING.rx);
  const wing = { x: cos * 0.42 * RING.rx, y: sin * 0.42 * RING.rx * Math.sqrt(FLAT) };
  const pinLen = level ? 0 : Math.max(-24, Math.min(24, reading.heightM * 4));
  const pin = (
    <>
      <path
        d={`M${tip.x} ${tip.y} L${tip.x} ${tip.y - pinLen}`}
        stroke={accent}
        strokeWidth="2"
        strokeLinecap="round"
      />
      <circle cx={tip.x} cy={tip.y - pinLen} r="2.5" fill={accent} stroke="#1a1816" />
    </>
  );

  const distance = arrived ? 'HERE' : `${metres(reading.distM)} m`;
  const height = arrived
    ? level
      ? 'level'
      : reading.heightM > 0
        ? 'look up'
        : 'look down'
    : level
      ? 'level'
      : `${metres(Math.abs(reading.heightM))} m ${reading.heightM > 0 ? 'above' : 'below'}`;
  const heightColour = arrived ? ORANGE : level || reading.heightM < 0 ? TAN_TEXT : CREAM;

  return (
    <svg viewBox="0 0 234 102" aria-hidden="true">
      <rect x="1" y="1" width="232" height="100" rx="4" fill="#000" opacity="0.6" />
      <rect x="0" y="0" width="232" height="100" rx="4" fill="#2e2b2a" />
      {/* Below you: the pin hangs under the floor, so the ring is drawn over it. */}
      {pinLen < 0 && pin}
      <path d="M16 50 A40 18 0 0 1 96 50" fill="none" stroke={TAN_DARK} strokeWidth="2.5" strokeLinecap="round" />
      <path d="M56 32 L56 26" stroke={TAN_DARK} strokeWidth="2" strokeLinecap="round" />
      <path d="M16 50 A40 18 0 0 0 96 50" fill="none" stroke={accent} strokeWidth="2.5" strokeLinecap="round" />
      {!arrived && (
        <path
          d={`M${tip.x} ${tip.y} L${base.x + wing.x} ${base.y + wing.y} L${RING.cx} ${RING.cy} L${base.x - wing.x} ${base.y - wing.y} Z`}
          fill={CREAM}
        />
      )}
      {pinLen >= 0 && pin}
      <text className="big" x="112" y="44" fill={accent}>
        {distance}
      </text>
      <text className="small" x="112" y="70" fill={heightColour}>
        {height}
      </text>
    </svg>
  );
}

function Slider({
  label,
  value,
  min,
  max,
  step,
  format,
  onInput,
  onCommit
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  onInput: (value: number) => void;
  onCommit: (value: number) => void;
}) {
  // The value being dragged. It feeds the preview as it moves and is sent once the drag or key press ends.
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? value;
  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    onCommit(draft);
  };
  return (
    <>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={shown}
        onChange={(e) => {
          setDraft(e.target.valueAsNumber);
          onInput(e.target.valueAsNumber);
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="val">{format(shown)}</span>
    </>
  );
}

function Row({ label, tone, children }: { label: string; tone?: 'idle' | 'bad'; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={tone}>
        {tone === 'bad' && <TriangleAlert aria-hidden="true" />}
        {children}
      </dd>
    </>
  );
}

const time = (at: number) => new Date(at).toLocaleTimeString(undefined, { hour12: false });

function CopyLine({ text }: { text: string }) {
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle');
  useEffect(() => {
    if (state === 'idle') return;
    const timer = setTimeout(() => setState('idle'), 1200);
    return () => clearTimeout(timer);
  }, [state]);
  return (
    <div className="copy">
      <code>{text}</code>
      <button
        type="button"
        onClick={() =>
          navigator.clipboard.writeText(text).then(
            () => setState('copied'),
            () => setState('failed')
          )
        }
      >
        {state === 'copied' ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
        {state === 'idle' ? 'Copy' : state === 'copied' ? 'Copied' : "Couldn't copy"}
      </button>
    </div>
  );
}

// The radar tool. Row 1 is now (the overlay preview beside the live status, or the setup steps on first run),
// row 2 is change (how the overlay looks, how the radar behaves).
export function RadarView({
  className,
  radar,
  game,
  onEnable,
  onChange,
  onMove,
  onReset,
  onOpenSettings
}: RadarViewProps) {
  const { settings, status, setupDone } = radar;
  // Slider values mid-drag drive the preview before they are saved.
  const [look, setLook] = useState<{ opacity?: number; scale?: number }>({});
  const [portDraft, setPortDraft] = useState<string | null>(null);
  const [showSetup, setShowSetup] = useState(false);
  const setup = !setupDone || showSetup;

  // The steps fold into the live rows as soon as RCON first connects, also when they were reopened.
  useEffect(() => setShowSetup(false), [setupDone]);

  const opacity = look.opacity ?? settings.opacity;
  const scale = look.scale ?? settings.scale;
  const reading = status.reading ?? SAMPLE;
  const connected = status.link === 'connected';
  const tf2Found = game?.tf2Found ?? true;

  function savePort() {
    const port = Number(portDraft);
    setPortDraft(null);
    if (!Number.isInteger(port) || port < 1024 || port > 65535 || port === settings.port) return;
    onChange({ port });
  }

  const link: ReactNode =
    status.link === 'connected' ? (
      `Connected on port ${settings.port}`
    ) : status.link === 'unreachable' ? (
      'Refused: TF2 is running without the launch options'
    ) : status.link === 'wrong-password' ? (
      'Wrong password: restart TF2 with the current launch options'
    ) : status.link === 'waiting' && status.tf2Running ? (
      'Connecting'
    ) : (
      <span className="idle">{status.link === 'off' ? 'Off' : 'Waiting for TF2'}</span>
    );
  const linkBad = status.link === 'unreachable' || status.link === 'wrong-password';

  return (
    <section id="view-radar" className={className} role="tabpanel" aria-labelledby="tab-radar">
      <main className="wrap radar">
        {!tf2Found && (
          <div className="banner" role="status">
            <TriangleAlert aria-hidden="true" />
            <p>TF2 was not found in your Steam libraries, so the radar can't follow it.</p>
            <button type="button" className="btn small" onClick={onOpenSettings}>
              Choose folder
            </button>
          </div>
        )}

        <div className="radar-top">
          <span>Radar</span>
          <Switch big label="Radar" checked={settings.enabled} disabled={!tf2Found} onChange={onEnable} />
        </div>

        <div className="radar-grid">
          <div className="anchor">
            <div className="anchor-stage">
              <div style={{ opacity, transform: `scale(${scale})` }}>
                <Panel reading={reading} />
              </div>
            </div>
            <p className="cap">
              {status.reading ? (
                <>
                  Live:{' '}
                  <strong>{`${metres(reading.distM)} m, ${metres(Math.abs(reading.heightM))} m ${reading.heightM >= 0 ? 'above' : 'below'}`}</strong>
                </>
              ) : (
                'Sample reading'
              )}
              {` · ${Math.round(opacity * 100)}% opacity, ${scale.toFixed(2)}x`}
            </p>
          </div>

          {setup ? (
            <div className="setup">
              <p className="kicker">Set up once</p>
              <h3>Add the launch options to TF2</h3>
              <ol className="steps">
                <li>
                  <strong>Copy this line.</strong>
                  <CopyLine text={radar.launchOptions} />
                  <p className="note">
                    <code>-usercon</code> turns on TF2's remote console, limited to this PC by the whitelist and a
                    password made for you. <code>-condebug</code> writes the console log that match tracking reads. The
                    radar only sends <code>getpos</code>, <code>record</code> and empty <code>alias</code> commands that
                    make TF2 save the demo sooner.
                  </p>
                </li>
                <li>
                  In Steam: right-click <strong>Team Fortress 2</strong>, <strong>Properties</strong>,{' '}
                  <strong>Launch Options</strong>. Keep anything already there.
                </li>
                <li>
                  <strong>Restart TF2</strong> in Windowed (no border) mode. The live status takes this spot once the
                  radar connects.
                </li>
              </ol>
              {linkBad ? (
                <p className="state bad">
                  <TriangleAlert aria-hidden="true" />
                  {status.link === 'unreachable'
                    ? 'TF2 is running without these options. Restart it after adding the line.'
                    : 'TF2 is running with an old password. Restart it with the line above.'}
                </p>
              ) : !settings.enabled ? (
                <p className="state warn">The radar is off. Turn it on to connect.</p>
              ) : setupDone ? (
                <p className="state ok">
                  Applied. RCON is reachable from this PC only.
                  <span className="spacer" />
                  <button type="button" className="link muted" onClick={() => setShowSetup(false)}>
                    Back to status
                  </button>
                </p>
              ) : null}
            </div>
          ) : (
            <div>
              <dl className="rows">
                <Row label="TF2" tone={status.tf2Running ? undefined : 'idle'}>
                  {status.tf2Running ? 'Running' : 'Not running'}
                </Row>
                <Row label="RCON" tone={linkBad ? 'bad' : undefined}>
                  {link}
                </Row>
                <Row label="Position" tone={status.positionHz === null ? 'idle' : undefined}>
                  {status.positionHz === null ? (
                    connected ? (
                      'Starting'
                    ) : (
                      'Waiting for RCON'
                    )
                  ) : (
                    <>
                      Reading <code>getpos</code> over RCON <small>· {Math.round(status.positionHz)} per second</small>
                    </>
                  )}
                </Row>
                <Row label="Demo" tone={status.demo ? undefined : 'idle'}>
                  {status.demo ? (
                    <>
                      Following <code>{status.demo.file}</code>{' '}
                      <small>
                        ·{' '}
                        {status.demo.intervalS === null
                          ? 'measuring the write interval'
                          : `written every ${status.demo.intervalS.toFixed(1)} s`}
                        , filler {Math.round(status.demo.fillerKbps)} KB/s
                      </small>
                    </>
                  ) : (
                    'Not recording'
                  )}
                </Row>
                <Row label="Gargoyle" tone={status.gargoyle ? undefined : 'idle'}>
                  {status.gargoyle ? (
                    <>
                      <strong className="ok">Found</strong> on {status.gargoyle.map ?? 'an unknown map'} at (
                      {Math.round(status.gargoyle.x)}, {Math.round(status.gargoyle.y)}, {Math.round(status.gargoyle.z)})
                    </>
                  ) : (
                    'None yet'
                  )}
                </Row>
              </dl>
              {radar.events.length === 0 ? (
                <p className="note events-empty">No events yet. Start TF2 to connect.</p>
              ) : (
                <ul className="events">
                  {radar.events.slice(0, 4).map((event) => (
                    <li key={`${event.at}:${event.text}`} className={event.tone}>
                      <time dateTime={new Date(event.at).toISOString()}>{time(event.at)}</time>
                      <span>{event.text}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}

          <Group title="Overlay look">
            <Field name="Opacity">
              <Slider
                label="Overlay opacity"
                value={Math.round(settings.opacity * 100)}
                min={20}
                max={100}
                step={2}
                format={(v) => `${v}%`}
                onInput={(v) => setLook({ ...look, opacity: v / 100 })}
                onCommit={(v) => {
                  setLook({});
                  if (v / 100 !== settings.opacity) onChange({ opacity: v / 100 });
                }}
              />
            </Field>
            <Field name="Scale">
              <Slider
                label="Overlay scale"
                value={settings.scale}
                min={0.5}
                max={2.5}
                step={0.05}
                format={(v) => `${v.toFixed(2)}x`}
                onInput={(v) => setLook({ ...look, scale: v })}
                onCommit={(v) => {
                  setLook({});
                  if (v !== settings.scale) onChange({ scale: v });
                }}
              />
            </Field>
            <Field
              name="Position"
              help={
                status.editing
                  ? `Editing in game: drag the panel, drag its corner to resize. ${settings.editKey.name} saves.`
                  : 'Move radar opens edit mode in game: drag the panel, drag its corner to resize.'
              }
            >
              <button
                type="button"
                className="btn small"
                disabled={!connected || status.editing}
                title={connected ? undefined : 'Start TF2 with the launch options first'}
                onClick={onMove}
              >
                Move radar
              </button>
              <button type="button" className="btn small" onClick={onReset}>
                Reset layout
              </button>
            </Field>
            <Field
              name="In game"
              help={
                <>
                  <kbd>{settings.toggleKey.name}</kbd> turns the radar off and on. <kbd>{settings.editKey.name}</kbd>{' '}
                  starts and saves edit mode. Change them in Settings.
                </>
              }
            />
          </Group>

          <Group title="Behaviour">
            <Field
              name="Demo delay"
              help="Longest wait before a new gargoyle shows. Lower is faster, but the demo file grows faster: at 5 s up to about 500 MB an hour on a quiet server."
            >
              <Slider
                label="Demo delay in seconds"
                value={settings.demoDelayS}
                min={2}
                max={20}
                step={1}
                format={(v) => `${v} s`}
                onInput={() => {}}
                onCommit={(demoDelayS) => {
                  if (demoDelayS !== settings.demoDelayS) onChange({ demoDelayS });
                }}
              />
            </Field>
            <Field
              name="RCON port"
              help={
                setupDone ? (
                  <>
                    Launch options applied.{' '}
                    {!setup && (
                      <button type="button" className="link muted" onClick={() => setShowSetup(true)}>
                        Show launch options
                      </button>
                    )}
                  </>
                ) : (
                  <span className="warn">Launch options not applied yet.</span>
                )
              }
            >
              <input
                type="number"
                aria-label="RCON port"
                min={1024}
                max={65535}
                value={portDraft ?? settings.port}
                onChange={(e) => setPortDraft(e.target.value)}
                onBlur={savePort}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') e.currentTarget.blur();
                }}
              />
            </Field>
            <Field
              name="Follow any gargoyle"
              help={
                <>
                  Test mode for local servers: <code>sv_cheats 1</code>, then{' '}
                  <code>ent_create tf_halloween_gift_pickup</code>.
                </>
              }
            >
              <input
                type="checkbox"
                aria-label="Follow any gargoyle"
                checked={settings.anyGargoyle}
                onChange={(e) => onChange({ anyGargoyle: e.target.checked })}
              />
            </Field>
          </Group>
        </div>
      </main>
    </section>
  );
}
