import { Settings as SettingsIcon } from 'lucide-react';
import { useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';

export type ViewId = 'contracts' | 'radar' | 'settings';

/** One line under the tool name. The tone adds the tab's only indicator: green pulsing = live, orange = needs you. */
export type TabStatus = { text: string; tone?: 'live' | 'warn' };

export type Tool = { id: ViewId; label: string; status: TabStatus };

type TabNavProps = {
  tools: Tool[];
  active: ViewId;
  onSelect: (id: ViewId) => void;
};

// The shell's tab strip: tool tabs on the left, Settings on the right, one sliding underline.
export function TabNav({ tools, active, onSelect }: TabNavProps) {
  const nav = useRef<HTMLElement>(null);
  const [ink, setInk] = useState({ left: 0, width: 0 });
  const ids: ViewId[] = [...tools.map((tool) => tool.id), 'settings'];

  // The underline follows the selected tab. Re-measure when the window resizes or the tab does (font load).
  useLayoutEffect(() => {
    const strip = nav.current;
    const tab = strip?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (!strip || !tab) return;
    const measure = () => setInk({ left: tab.offsetLeft, width: tab.offsetWidth });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(strip);
    observer.observe(tab);
    return () => observer.disconnect();
  }, [active, tools.length]);

  function onKeyDown(e: KeyboardEvent) {
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = ids[(ids.indexOf(active) + step + ids.length) % ids.length];
    onSelect(next);
    nav.current?.querySelector<HTMLButtonElement>(`#tab-${next}`)?.focus();
  }

  const tabProps = (id: ViewId) => ({
    id: `tab-${id}`,
    role: 'tab',
    'aria-selected': active === id,
    'aria-controls': `view-${id}`,
    tabIndex: active === id ? 0 : -1,
    onClick: () => onSelect(id)
  });

  return (
    <nav ref={nav} className="tabs" role="tablist" aria-label="Tools" onKeyDown={onKeyDown}>
      {tools.map(({ id, label, status }) => (
        <button key={id} type="button" className="tab" {...tabProps(id)}>
          <span className="tab-name">{label}</span>
          <span className="tab-status">
            {status.tone && <span className={`tab-dot ${status.tone}`} aria-hidden="true" />}
            {status.text}
          </span>
        </button>
      ))}
      <button type="button" className="tab is-settings" {...tabProps('settings')}>
        <SettingsIcon aria-hidden="true" />
        <span className="tab-name">Settings</span>
      </button>
      <span className="tab-ink" style={{ left: ink.left, width: ink.width }} aria-hidden="true" />
    </nav>
  );
}
