import { useId, type ButtonHTMLAttributes, type ReactNode } from 'react';

const cx = (...parts: (string | false | undefined)[]): string => parts.filter(Boolean).join(' ');

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary';
}

export function Button({
  variant = 'secondary',
  className,
  type = 'button',
  ...rest
}: ButtonProps) {
  return (
    <button type={type} className={cx('pv-button', `pv-button--${variant}`, className)} {...rest} />
  );
}

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  /** Accessible name (already translated). Also used as tooltip. */
  label: string;
  icon: ReactNode;
  /** When defined, the button behaves as a toggle (`aria-pressed`). */
  active?: boolean;
}

export function IconButton({
  label,
  icon,
  active,
  className,
  type = 'button',
  ...rest
}: IconButtonProps) {
  return (
    <button
      type={type}
      aria-label={label}
      title={label}
      aria-pressed={active}
      className={cx('pv-icon-button', className)}
      {...rest}
    >
      {icon}
    </button>
  );
}

export interface PanelProps {
  title: string;
  children: ReactNode;
  className?: string;
}

export function Panel({ title, children, className }: PanelProps) {
  const id = useId();
  return (
    <section className={cx('pv-panel', className)} aria-labelledby={id}>
      <h2 id={id} className="pv-panel__title">
        {title}
      </h2>
      {children}
    </section>
  );
}

export interface SelectOption<T extends string> {
  value: T;
  label: string;
}

export interface SelectProps<T extends string> {
  label: string;
  value: T;
  options: readonly SelectOption<T>[];
  onChange: (value: T) => void;
}

export function Select<T extends string>({ label, value, options, onChange }: SelectProps<T>) {
  const id = useId();
  return (
    <div className="pv-field">
      <label htmlFor={id} className="pv-field__label">
        {label}
      </label>
      <select
        id={id}
        className="pv-select"
        value={value}
        onChange={(e) => {
          const next = options.find((o) => o.value === e.target.value);
          if (next) onChange(next.value);
        }}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </div>
  );
}

export interface ToggleProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}

export function Toggle({ label, checked, onChange }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      className="pv-toggle"
      onClick={() => {
        onChange(!checked);
      }}
    >
      <span className="pv-toggle__track" aria-hidden="true">
        <span className="pv-toggle__thumb" />
      </span>
      <span>{label}</span>
    </button>
  );
}

export interface StatProps {
  label: string;
  value: ReactNode;
}

export function Stat({ label, value }: StatProps) {
  return (
    <div className="pv-stat">
      <span className="pv-stat__label">{label}</span>
      <span className="pv-stat__value">{value}</span>
    </div>
  );
}
