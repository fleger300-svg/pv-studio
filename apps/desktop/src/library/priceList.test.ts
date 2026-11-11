import ExcelJS from 'exceljs';
import { describe, expect, it } from 'vitest';
import { base64ToBytes, readPriceList } from './priceList';

/** A synthetic K2 Base material list (French headers, scaled prices). */
async function k2List(): Promise<ArrayBuffer> {
  const wb = new ExcelJS.Workbook();
  const header = [
    'Art. no',
    "Description de l'article",
    'Nombre',
    'Poids',
    "Nombre d'unités de vente",
    'Quantité par unité de vente',
    'Prix catalogue',
  ];
  const total = wb.addWorksheet('Total');
  total.addRow(header);
  total.addRow(['2004393', 'SingleRail 36; 4.80 m', 128, 472.576, 7, 20, 57.09]);
  // A formula keeps its cached result.
  total.addRow([
    2004148,
    { richText: [{ text: 'K2 Clamp ' }, { text: 'MC 25-40 Black' }] },
    112,
    7.3136,
    6,
    20,
    { formula: 'A1', result: 3.44 },
  ]);
  wb.addWorksheet('Vide');
  return await wb.xlsx.writeBuffer();
}

describe('readPriceList', () => {
  it('reads every sheet as raw cells', async () => {
    const sheets = await readPriceList(ExcelJS, await k2List());
    expect(sheets.map((s) => s.name)).toEqual(['Total', 'Vide']);
    const [header, rail, clamp] = sheets[0]?.rows ?? [];
    expect(header?.[0]).toEqual({ kind: 'text', value: 'Art. no' });
    expect(rail?.[6]).toEqual({ kind: 'number', value: 57.09 });
    expect(clamp?.[0]).toEqual({ kind: 'number', value: 2004148 });
    expect(clamp?.[1]).toEqual({ kind: 'text', value: 'K2 Clamp MC 25-40 Black' });
    expect(clamp?.[6]).toEqual({ kind: 'number', value: 3.44 });
    expect(sheets[1]?.rows).toEqual([]);
  });

  it('decodes base64', () => {
    expect(Array.from(base64ToBytes(btoa('PV')))).toEqual([80, 86]);
  });
});
