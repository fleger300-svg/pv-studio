/** Characters grouping thousands: spaces, no-break spaces (U+00A0, U+202F), apostrophes. */
const GROUPING = new RegExp(`[\\s${String.fromCharCode(0xa0, 0x202f)}']`, 'g');

/**
 * Number entry in the user's locale: « 6,5 » and « 6.5 » both read 6.5, thousands
 * separators are ignored. `null` for anything else.
 */
export function parseNumber(text: string): number | null {
  const s = text.replace(GROUPING, '').replace(',', '.');
  if (!/^[+-]?(\d+\.?\d*|\.\d+)$/.test(s)) return null;
  const v = Number(s);
  return Number.isFinite(v) ? v : null;
}

/** A number for an input field: at most `decimals` decimals, trailing zeros dropped. */
export function formatNumber(value: number, locale: string, decimals: number): string {
  return new Intl.NumberFormat(locale, {
    maximumFractionDigits: decimals,
    useGrouping: false,
  }).format(value);
}

export type Validation =
  { ok: true; value: number } | { ok: false; error: string; params: Record<string, number> };

/** Parses and checks a field entry against its range. */
export function validateNumber(text: string, min: number, max: number): Validation {
  const v = parseNumber(text);
  if (v === null) return { ok: false, error: 'forms.notANumber', params: {} };
  if (v < min || v > max) return { ok: false, error: 'forms.outOfRange', params: { min, max } };
  return { ok: true, value: v };
}
