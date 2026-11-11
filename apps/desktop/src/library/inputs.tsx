/// Draft inputs of the library forms: values are edited locally and stored only by « Save »
/// (a library change is one new revision, not one command per field).
import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { formatNumber } from '../forms/numbers';

export function TextInput({
  label,
  value,
  onChange,
  required = false,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  required?: boolean;
}) {
  const id = useId();
  return (
    <div className="pv-field form-field">
      <label htmlFor={id} className="pv-field__label">
        {label}
      </label>
      <input
        id={id}
        className="form-input"
        value={value}
        required={required}
        aria-invalid={required && value.trim() === ''}
        onChange={(e) => {
          onChange(e.target.value);
        }}
      />
    </div>
  );
}

/** A number typed in the user's locale (comma or point), applied when valid. */
export function NumInput({
  label,
  value,
  unit,
  decimals = 3,
  onChange,
  min = -Infinity,
  max = Infinity,
}: {
  label: string;
  value: number | null;
  unit?: string;
  decimals?: number;
  onChange: (v: number | null) => void;
  min?: number;
  max?: number;
}) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? (value === null ? '' : formatNumber(value, i18n.language, decimals));
  const parsed =
    draft === null
      ? value
      : draft.trim() === ''
        ? null
        : Number(draft.replace(/\s/g, '').replace(',', '.'));
  const invalid = parsed !== null && (Number.isNaN(parsed) || parsed < min || parsed > max);
  return (
    <div className="pv-field form-field">
      <label htmlFor={id} className="pv-field__label">
        {label}
        {unit ? ` (${unit})` : ''}
      </label>
      <input
        id={id}
        className={invalid ? 'form-input form-input--error' : 'form-input'}
        inputMode="decimal"
        value={shown}
        aria-invalid={invalid}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onBlur={() => {
          if (!invalid) onChange(parsed);
          setDraft(null);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          if (e.key === 'Escape') setDraft(null);
        }}
      />
      {invalid && (
        <span className="form-field__error" role="alert">
          {t('library.invalidNumber')}
        </span>
      )}
    </div>
  );
}
