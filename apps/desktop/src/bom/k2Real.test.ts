import ExcelJS from 'exceljs';
import { describe, expect, it } from 'vitest';
import { createI18n } from '../i18n/config';
import fixture from '../../../../testdata/fixtures/bom-k2.json';
import { buildWorkbookModel, type Translate } from './k2Format';
import { writeWorkbook } from './xlsx';

/**
 * Optional check against the real K2 Base workbook of project A, which stays out of git:
 * set `K2_XLSX=<path to the .xlsx>` to run it. Compares our export with K2's tab by tab:
 * header row word for word; per article: quantity, mass, sale units, pack. Prices are left
 * out (the reference prices are scaled).
 */
const K2_XLSX = (globalThis as { process?: { env: Record<string, string | undefined> } }).process
  ?.env.K2_XLSX;

const fr = createI18n('fr');
const t: Translate = (key, params) => fr.t(key, params);

type Row = { article: string; quantity: number; mass: number; units: number; pack: number };

function rows(ws: ExcelJS.Worksheet): Row[] {
  const out: Row[] = [];
  ws.eachRow((r, i) => {
    if (i === 1) return;
    const v = (c: number) => r.getCell(c).value;
    if (v(3) === null) return; // blank row / notes
    out.push({
      article: r.getCell(1).text,
      quantity: Number(v(3)),
      mass: Number(v(4)),
      units: Number(v(5)),
      pack: Number(v(6)),
    });
  });
  return out.sort((a, b) => a.article.localeCompare(b.article));
}

describe.skipIf(!K2_XLSX)('export against the real K2 workbook (K2_XLSX)', () => {
  it('matches K2 tab by tab', async () => {
    const k2 = new ExcelJS.Workbook();
    await k2.xlsx.readFile(K2_XLSX ?? '');
    const ours = new ExcelJS.Workbook();
    await ours.xlsx.load(await writeWorkbook(ExcelJS, buildWorkbookModel(fixture, 'fr', t)));
    expect(ours.worksheets).toHaveLength(k2.worksheets.length);
    expect(ours.worksheets[0]?.name).toBe(k2.worksheets[0]?.name); // « Total »
    k2.worksheets.forEach((k, i) => {
      const o = ours.worksheets[i];
      if (!o) throw new Error(`tab ${String(i)}`);
      expect((o.getRow(1).values as unknown[]).slice(1)).toEqual(
        (k.getRow(1).values as unknown[]).slice(1),
      );
      const [ko, oo] = [rows(k), rows(o)];
      expect(oo.map((r) => r.article)).toEqual(ko.map((r) => r.article));
      ko.forEach((kr, j) => {
        const or = oo[j];
        if (!or) throw new Error(kr.article);
        expect([or.quantity, or.units, or.pack], `${k.name} ${kr.article}`).toEqual([
          kr.quantity,
          kr.units,
          kr.pack,
        ]);
        expect(Math.abs(or.mass - kr.mass)).toBeLessThan(0.001);
      });
    });
  });
});
