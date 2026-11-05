import { describe, expect, it } from 'vitest';
import fr from '../fr.json';
import en from '../en.json';
import de from '../de.json';
import { checkParity } from './parity.ts';

describe('locale key parity', () => {
  it('fr, en and de have exactly the same keys and no "ß"', () => {
    expect(checkParity({ fr, en, de })).toEqual([]);
  });

  it('detects missing, extra and forbidden entries', () => {
    const issues = checkParity({
      en: { a: 'A', b: { c: 'C' } },
      fr: { a: 'A', x: 'X' },
      de: { a: 'Strasse', b: { c: 'Straße' } },
    });
    expect(issues).toEqual(
      expect.arrayContaining([
        { locale: 'fr', kind: 'missing', key: 'b.c' },
        { locale: 'fr', kind: 'extra', key: 'x' },
        { locale: 'de', kind: 'forbiddenChar', key: 'b.c' },
      ]),
    );
    expect(issues).toHaveLength(3);
  });
});
