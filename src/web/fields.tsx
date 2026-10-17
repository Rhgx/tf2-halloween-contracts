import type { ReactNode } from 'react';

// One level of grouping: a heading on a rule line, then rows on the page background.
export function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="group">
      <div className="group-head">
        <h3>{title}</h3>
      </div>
      {children}
    </div>
  );
}

export function Field({
  name,
  help,
  wide,
  children
}: {
  name: string;
  help?: ReactNode;
  wide?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className={wide ? 'field wide' : 'field'}>
      <span className="name">{name}</span>
      {help && <span className="help">{help}</span>}
      {children && <span className="control">{children}</span>}
    </div>
  );
}

export function Switch({
  label,
  checked,
  disabled,
  big,
  onChange
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  big?: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <button
      type="button"
      className={big ? 'toggle big' : 'toggle'}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}
