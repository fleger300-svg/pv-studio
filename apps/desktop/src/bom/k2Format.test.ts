import { describe, expect, it } from 'vitest';
import type { Bom_Serialize } from '../bindings';
import { createI18n } from '../i18n/config';
import fixture from '../../../../testdata/fixtures/bom-k2.json';
import reference from '../../../../testdata/k2/reference_bom.json';
import { buildWorkbookModel, sheetNames, type Translate } from './k2Format';
import { K2_HEADERS } from './k2Headers';

const bom = fixture as unknown as Bom_Serialize;
const fr = createI18n('fr');
const t: Translate = (key, params) => fr.t(key, params);

describe('K2 workbook model', () => {
  const model = buildWorkbookModel(bom, 'fr', t);

  it('has K2 tabs in K2 order and K2 headers in French', () => {
    expect(model.sheets.map((s) => s.name)).toEqual([
      'Total',
      'roof-1',
      'roof-2',
      'roof-3',
      'roof-4',
    ]);
    for (const s of model.sheets) expect(s.header).toEqual(K2_HEADERS);
  });

  it('reproduces every line of the K2 list', () => {
    const tabs = [reference.total, ...reference.roofs];
    const packs = new Map(reference.articles.map((a) => [a.article_no, a.pack_qty]));
    model.sheets.forEach((sheet, i) => {
      const expected = tabs[i]?.lines ?? [];
      expect(sheet.rows).toHaveLength(expected.length);
      for (const e of expected) {
        const row = sheet.rows.find((r) => r[0] === e.article_no);
        expect(row, `${sheet.name} ${e.article_no}`).toBeDefined();
        if (!row) continue;
        const [, , quantity, mass, saleUnits, pack, , rate, , price] = row;
        expect(quantity).toBe(e.quantity);
        expect(mass as number).toBeCloseTo(e.mass_kg, 3);
        expect(saleUnits).toBe(e.sale_units);
        expect(pack).toBe(packs.get(e.article_no));
        expect(rate).toBe(0);
        expect(Math.abs((price as number) - e.price)).toBeLessThan(0.005);
      }
    });
  });

  it('writes the saved-bars line below the Total table only', () => {
    const withSaved: Bom_Serialize = {
      ...bom,
      savedBars: [{ articleNo: '2004393', perFaces: 130, project: 128 }],
    };
    const m = buildWorkbookModel(withSaved, 'fr', t);
    expect(m.sheets[0]?.notes).toEqual([
      'Barres économisées par réutilisation des chutes : article 2004393, 130 barres pan par pan, 128 sur le projet (2 de moins).',
    ]);
    expect(m.sheets.slice(1).every((s) => s.notes.length === 0)).toBe(true);
  });

  it('uses the translated description, else the manufacturer label', () => {
    const line = bom.total.lines[0];
    if (!line) throw new Error('fixture');
    const translated = { ...line, descriptions: { de: 'Stehfalzklemme' } };
    const m = buildWorkbookModel(
      { ...bom, total: { ...bom.total, lines: [translated] }, faces: [] },
      'de',
      t,
    );
    expect(m.sheets[0]?.rows[0]?.[1]).toBe('Stehfalzklemme');
    expect(buildWorkbookModel(bom, 'de', t).sheets[0]?.rows[0]?.[1]).toBe(line.label);
  });
});

describe('sheet names', () => {
  it('are valid and unique Excel names', () => {
    expect(sheetNames(['Total', 'Pan [Sud]/Est', 'Pan [Sud]/Est', 'x'.repeat(40)])).toEqual([
      'Total',
      'Pan -Sud--Est',
      'Pan -Sud--Est (2)',
      'x'.repeat(31),
    ]);
  });
});
