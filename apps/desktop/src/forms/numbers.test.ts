import { describe, expect, it } from 'vitest';
import { formatNumber, parseNumber, validateNumber } from './numbers';

describe('numbers', () => {
  it('reads both decimal separators and ignores grouping', () => {
    expect(parseNumber('6,5')).toBe(6.5);
    expect(parseNumber(' 6.5 ')).toBe(6.5);
    expect(parseNumber('1 234,5')).toBe(1234.5);
    expect(parseNumber("1'234")).toBe(1234);
    expect(parseNumber('-0,25')).toBe(-0.25);
    expect(parseNumber('')).toBeNull();
    expect(parseNumber('6,5,1')).toBeNull();
    expect(parseNumber('abc')).toBeNull();
  });

  it('formats in the locale without trailing zeros', () => {
    expect(formatNumber(6.5, 'fr', 2)).toBe('6,5');
    expect(formatNumber(30, 'fr', 1)).toBe('30');
    expect(formatNumber(1234.567, 'en', 2)).toBe('1234.57');
  });

  it('validates ranges', () => {
    expect(validateNumber('35', 1, 89)).toEqual({ ok: true, value: 35 });
    expect(validateNumber('95', 1, 89)).toEqual({
      ok: false,
      error: 'forms.outOfRange',
      params: { min: 1, max: 89 },
    });
    expect(validateNumber('x', 1, 89).ok).toBe(false);
  });
});
