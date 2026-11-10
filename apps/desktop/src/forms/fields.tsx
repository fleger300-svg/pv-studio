import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { formatNumber, validateNumber } from './numbers';

/**
 * Fields that commit: the user types freely, Enter or leaving the field validates and
 * sends one command (`onCommit`), Escape cancels. While editing, the field shows the
 * draft; otherwise it shows the model's value — so a refused command (Rust said no, a
 * toast explains why) simply shows the old value again.
 */

interface CommitState {
  draft: string | null;
  error: { key: string; params: Record<string, number> } | null;
  pending: boolean;
}

function useCommit(): [CommitState, React.Dispatch<React.SetStateAction<CommitState>>] {
  return useState<CommitState>({ draft: null, error: null, pending: false });
}

export interface NumberFieldProps {
  label: string;
  /** Model value (`null`: empty, e.g. « the building's eave height »). */
  value: number | null;
  unit?: string;
  min: number;
  max: number;
  decimals?: number;
  /** Shown when the value is `null`. */
  placeholder?: string;
  /** An empty entry commits `null` (optional values). */
  allowEmpty?: boolean;
  disabled?: boolean;
  /** Sends the command; `false` when it was refused. */
  onCommit: (value: number | null) => Promise<boolean>;
  /** Hover / focus highlight hook (e.g. an edge on the plan). */
  onFocusChange?: (focused: boolean) => void;
}

export function NumberField({
  label,
  value,
  unit,
  min,
  max,
  decimals = 2,
  placeholder,
  allowEmpty = false,
  disabled = false,
  onCommit,
  onFocusChange,
}: NumberFieldProps) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const [state, setState] = useCommit();
  const shown = state.draft ?? (value === null ? '' : formatNumber(value, i18n.language, decimals));

  const commit = async () => {
    const text = state.draft;
    if (text === null) return;
    if (allowEmpty && text.trim() === '') {
      setState({ draft: null, error: null, pending: true });
      if (value !== null) await onCommit(null);
      setState({ draft: null, error: null, pending: false });
      return;
    }
    const v = validateNumber(text, min, max);
    if (!v.ok) {
      setState({ draft: text, error: { key: v.error, params: v.params }, pending: false });
      return;
    }
    setState({ draft: null, error: null, pending: true });
    if (value === null || Math.abs(v.value - value) > 1e-9) await onCommit(v.value);
    setState({ draft: null, error: null, pending: false });
  };

  return (
    <div className="pv-field form-field">
      <label htmlFor={id} className="pv-field__label">
        {label}
      </label>
      <div className="form-field__row">
        <input
          id={id}
          className={state.error ? 'form-input form-input--error' : 'form-input'}
          inputMode="decimal"
          value={shown}
          placeholder={placeholder}
          disabled={disabled || state.pending}
          aria-invalid={state.error !== null}
          aria-busy={state.pending}
          onChange={(e) => {
            setState({ draft: e.target.value, error: null, pending: false });
          }}
          onFocus={() => onFocusChange?.(true)}
          onBlur={() => {
            onFocusChange?.(false);
            if (state.error) setState({ draft: null, error: null, pending: false });
            else void commit();
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void commit();
            if (e.key === 'Escape') setState({ draft: null, error: null, pending: false });
          }}
        />
        {unit && <span className="form-field__unit">{unit}</span>}
      </div>
      {state.error && (
        <span className="form-field__error" role="alert">
          {t(state.error.key, state.error.params)}
        </span>
      )}
    </div>
  );
}

export interface TextFieldProps {
  label: string;
  value: string;
  disabled?: boolean;
  onCommit: (value: string) => Promise<boolean>;
}

export function TextField({ label, value, disabled = false, onCommit }: TextFieldProps) {
  const { t } = useTranslation();
  const id = useId();
  const [state, setState] = useCommit();
  const shown = state.draft ?? value;
  const commit = async () => {
    const text = state.draft?.trim();
    if (text === undefined) return;
    if (text === '') {
      setState({
        draft: state.draft,
        error: { key: 'forms.required', params: {} },
        pending: false,
      });
      return;
    }
    setState({ draft: null, error: null, pending: true });
    if (text !== value) await onCommit(text);
    setState({ draft: null, error: null, pending: false });
  };
  return (
    <div className="pv-field form-field">
      <label htmlFor={id} className="pv-field__label">
        {label}
      </label>
      <input
        id={id}
        className={state.error ? 'form-input form-input--error' : 'form-input'}
        value={shown}
        disabled={disabled || state.pending}
        aria-invalid={state.error !== null}
        onChange={(e) => {
          setState({ draft: e.target.value, error: null, pending: false });
        }}
        onBlur={() => {
          if (state.error) setState({ draft: null, error: null, pending: false });
          else void commit();
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void commit();
          if (e.key === 'Escape') setState({ draft: null, error: null, pending: false });
        }}
      />
      {state.error && (
        <span className="form-field__error" role="alert">
          {t(state.error.key)}
        </span>
      )}
    </div>
  );
}

export interface CheckFieldProps {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onCommit: (checked: boolean) => Promise<boolean>;
}

/** A checkbox that commits on change. */
export function CheckField({ label, checked, disabled = false, onCommit }: CheckFieldProps) {
  // Optimistic: the new state shows while its command runs (the model's value afterwards,
  // the old one again if the command was refused).
  const [pending, setPending] = useState<boolean | null>(null);
  return (
    <label className="form-check">
      <input
        type="checkbox"
        checked={pending ?? checked}
        disabled={disabled || pending !== null}
        aria-busy={pending !== null}
        onChange={(e) => {
          setPending(e.target.checked);
          void onCommit(e.target.checked).finally(() => {
            setPending(null);
          });
        }}
      />
      <span>{label}</span>
    </label>
  );
}
