import { LogIn, RotateCcw, TriangleAlert } from 'lucide-react';
import { useState } from 'react';

import {
  DEFAULT_RADAR_KEYS,
  type GameState,
  type Radar,
  type ServerState,
  type Settings,
  type SettingsPatch
} from './api.ts';
import { Field, Group, Switch } from './fields.tsx';

type KeySlot = keyof typeof DEFAULT_RADAR_KEYS;

type SettingsViewProps = {
  className: string;
  settings: Settings | null;
  server: ServerState | null;
  game: GameState | null;
  syncedAt: string | null;
  /** Null where there is no radar (Linux); then the hotkeys group is not shown. */
  radar: Radar | null;
  onChange: (patch: SettingsPatch) => void;
  onChooseFolder: () => void;
  onUseDetected: () => void;
  onOpenData: () => void;
  onSignIn: () => void;
  onSignOut: () => void;
  onRetry: () => void;
  /** Resolves once the capture has ended, whether a key was taken, Esc cancelled it, or it failed (App toasts). */
  onCaptureKey: (which: KeySlot) => Promise<void>;
  onResetKey: (which: KeySlot) => void;
};

const PLATFORMS: Record<Settings['platform'], string | null> = { windows: 'Windows', linux: 'Linux', other: null };

// One hotkey: the key field (click it, then press the key anywhere) and a reset to its default beside it.
function HotkeyRow({
  name,
  which,
  radar,
  pending,
  onCapture,
  onReset
}: {
  name: string;
  which: KeySlot;
  radar: Radar;
  pending: KeySlot | null;
  onCapture: (which: KeySlot) => void;
  onReset: (which: KeySlot) => void;
}) {
  const key = which === 'toggle' ? radar.settings.toggleKey : radar.settings.editKey;
  const fallback = DEFAULT_RADAR_KEYS[which];
  const capturing = pending === which;
  return (
    <Field name={name}>
      <button
        type="button"
        className={capturing ? 'keyfield is-capturing' : 'keyfield'}
        aria-label={capturing ? 'Press a key. Esc cancels.' : `${name}: ${key.name}. Press to change.`}
        title={capturing ? 'Esc cancels' : undefined}
        disabled={pending !== null && !capturing}
        onClick={() => onCapture(which)}
      >
        {capturing ? 'Press a key' : key.name}
      </button>
      <button
        type="button"
        className="btn small icon"
        aria-label={`Reset to ${fallback.name}`}
        title={`Reset to ${fallback.name}`}
        disabled={pending !== null || key.code === fallback.code}
        onClick={() => onReset(which)}
      >
        <RotateCcw aria-hidden="true" />
      </button>
    </Field>
  );
}

// Things you set once. Changes apply as you make them; App rolls a failed one back.
export function SettingsView({
  className,
  settings,
  server,
  game,
  syncedAt,
  onChange,
  onChooseFolder,
  onUseDetected,
  onOpenData,
  onSignIn,
  onSignOut,
  onRetry,
  radar,
  onCaptureKey,
  onResetKey
}: SettingsViewProps) {
  const platform = settings && PLATFORMS[settings.platform];
  const tf2Folder = settings?.tf2Folder ?? settings?.tf2FolderDetected;
  // The hotkey being captured; one at a time, since the backend listens for a single key.
  const [pendingKey, setPendingKey] = useState<KeySlot | null>(null);

  function captureKey(which: KeySlot) {
    setPendingKey(which);
    onCaptureKey(which).finally(() => setPendingKey(null));
  }

  return (
    <section id="view-settings" className={className} role="tabpanel" aria-labelledby="tab-settings">
      <main className="wrap settings">
        {!settings ? (
          <p className="empty">
            Couldn't load the settings.{' '}
            <button type="button" className="link" onClick={onRetry}>
              Try again
            </button>
          </p>
        ) : (
          <>
            <Group title="App">
              <Field name="Close to tray">
                <Switch
                  label="Close to tray"
                  checked={settings.closeToTray}
                  onChange={(closeToTray) => onChange({ closeToTray })}
                />
              </Field>
              <Field name="Start minimized to tray">
                <Switch
                  label="Start minimized to tray"
                  checked={settings.startMinimized}
                  onChange={(startMinimized) => onChange({ startMinimized })}
                />
              </Field>
              {settings.platform === 'windows' && (
                <Field name="Start with Windows">
                  <Switch
                    label="Start with Windows"
                    checked={settings.startWithWindows}
                    onChange={(startWithWindows) => onChange({ startWithWindows })}
                  />
                </Field>
              )}
            </Group>

            <Group title="Game">
              <Field
                wide
                name="TF2 folder"
                help={!tf2Folder && 'Not found in your Steam libraries. Choose the folder that holds the tf folder.'}
              >
                <span className="path">
                  <code>{tf2Folder ?? 'Not found'}</code>
                  <button type="button" className="btn small" onClick={onChooseFolder}>
                    Change
                  </button>
                  <button
                    type="button"
                    className="btn small"
                    disabled={!settings.tf2FolderOverridden || !settings.tf2FolderDetected}
                    onClick={onUseDetected}
                  >
                    Use detected
                  </button>
                </span>
              </Field>
              <Field
                name="Match tracking"
                help={
                  game &&
                  !game.logFound && (
                    <>
                      Add <code>-condebug</code> to TF2's launch options in Steam.
                    </>
                  )
                }
              >
                {!game ? (
                  <span className="state">Unknown</span>
                ) : game.logFound ? (
                  <span className="state ok">console.log found</span>
                ) : (
                  <span className="state warn">
                    <TriangleAlert aria-hidden="true" />
                    console.log not found
                  </span>
                )}
              </Field>
            </Group>

            <Group title="Steam">
              <Field name="Account" help={server?.loggedIn && (syncedAt ? `Last sync ${syncedAt}` : 'Not synced yet')}>
                {server?.loggedIn ? (
                  <button type="button" className="btn small" onClick={onSignOut}>
                    Sign out
                  </button>
                ) : (
                  <button type="button" className="btn small" disabled={!server} onClick={onSignIn}>
                    <LogIn aria-hidden="true" />
                    Sign in with Steam
                  </button>
                )}
              </Field>
            </Group>

            <Group title="Contracts">
              <Field name="Restore original casual map selection on quit">
                <Switch
                  label="Restore original casual map selection on quit"
                  checked={settings.restoreQueueOnQuit}
                  onChange={(restoreQueueOnQuit) => onChange({ restoreQueueOnQuit })}
                />
              </Field>
            </Group>

            {radar && (
              <Group title="Radar hotkeys">
                <HotkeyRow
                  name="Turn the overlay on and off"
                  which="toggle"
                  radar={radar}
                  pending={pendingKey}
                  onCapture={captureKey}
                  onReset={onResetKey}
                />
                <HotkeyRow
                  name="Edit mode (move and resize)"
                  which="edit"
                  radar={radar}
                  pending={pendingKey}
                  onCapture={captureKey}
                  onReset={onResetKey}
                />
              </Group>
            )}

            <Group title="Data">
              <Field wide name="Data folder" help="Holds your Steam sign-in. Don't share it.">
                <span className="path">
                  <code>{settings.dataFolder}</code>
                  <button type="button" className="btn small" onClick={onOpenData}>
                    Open folder
                  </button>
                </span>
              </Field>
            </Group>

            <Group title="About">
              <Field name={`TF2 Halloween Utils ${settings.version}${platform ? ` on ${platform}` : ''}`}>
                <a
                  className="link muted"
                  href="https://github.com/Rhgx/tf2-halloween-utils"
                  target="_blank"
                  rel="noreferrer"
                >
                  GitHub
                </a>
              </Field>
            </Group>
          </>
        )}
      </main>
    </section>
  );
}
