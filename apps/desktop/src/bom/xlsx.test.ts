import ExcelJS from 'exceljs';
import { describe, expect, it } from 'vitest';
import type { Bom_Serialize } from '../bindings';
import { createI18n } from '../i18n/config';
import fixture from '../../../../testdata/fixtures/bom-k2.json';
import { buildWorkbookModel, type Translate } from './k2Format';
import { K2_HEADERS } from './k2Headers';
import { writeWorkbook } from './xlsx';

const fr = createI18n('fr');
const t: Translate = (key, params) => fr.t(key, params);

async function roundTrip(bom: Bom_Serialize): Promise<ExcelJS.Workbook> {
  const bytes = await writeWorkbook(ExcelJS, buildWorkbookModel(bom, 'fr', t));
  const wb = new ExcelJS.Workbook();
  await wb.xlsx.load(bytes);
  return wb;
}

describe('Excel export (written, then read back)', () => {
  it('keeps the K2 structure: tabs, headers, real numbers with formats', async () => {
    const bom = fixture as unknown as Bom_Serialize;
    const wb = await roundTrip(bom);
    expect(wb.worksheets.map((w) => w.name)).toEqual([
      'Total',
      'roof-1',
      'roof-2',
      'roof-3',
      'roof-4',
    ]);
    for (const ws of wb.worksheets) {
      const header = (ws.getRow(1).values as unknown[]).slice(1);
      expect(header).toEqual(K2_HEADERS);
    }
    const total = wb.getWorksheet('Total');
    if (!total) throw new Error('Total');
    // First data row: numbers, not strings, with the number formats.
    const row = total.getRow(2);
    expect(typeof row.getCell(1).value).toBe('string'); // article no.
    for (const c of [3, 4, 5, 6, 7, 8, 9, 10]) expect(typeof row.getCell(c).value).toBe('number');
    expect(row.getCell(10).numFmt).toBe('#,##0.00');
    expect(row.getCell(8).numFmt).toBe('0%');
    expect(total.rowCount).toBe(1 + bom.total.lines.length);
  });

  it('adds the saved-bars line after a blank row', async () => {
    const bom = fixture as unknown as Bom_Serialize;
    const wb = await roundTrip({
      ...bom,
      savedBars: [{ articleNo: '2004393', perFaces: 130, project: 128 }],
    });
    const total = wb.getWorksheet('Total');
    if (!total) throw new Error('Total');
    const last = total.getRow(total.rowCount).getCell(1).text;
    expect(last).toContain('(2 de moins)');
    expect(total.getRow(total.rowCount - 1).getCell(1).value).toBeNull();
  });
});
