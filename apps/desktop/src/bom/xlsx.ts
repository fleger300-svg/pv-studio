import type ExcelJS from 'exceljs';
import { COLUMNS, COLUMN_FORMATS, type WorkbookModel } from './k2Format';

/** Column widths (characters), K2-like. */
const WIDTHS = [10, 38, 9, 10, 12, 12, 12, 10, 12, 12];

/**
 * Writes the workbook model to `.xlsx` bytes. ExcelJS is passed in: the worker loads its
 * browser build, the tests its Node build.
 */
export async function writeWorkbook(
  excel: typeof ExcelJS,
  model: WorkbookModel,
): Promise<ArrayBuffer> {
  const wb = new excel.Workbook();
  wb.creator = 'PV Studio';
  for (const sheet of model.sheets) {
    const ws = wb.addWorksheet(sheet.name);
    ws.columns = COLUMNS.map((key, i) => ({
      key,
      width: WIDTHS[i] ?? 12,
      style: COLUMN_FORMATS[key] ? { numFmt: COLUMN_FORMATS[key] } : {},
    }));
    const header = ws.addRow(sheet.header);
    header.font = { bold: true };
    for (const r of sheet.rows) ws.addRow(r);
    if (sheet.notes.length > 0) {
      ws.addRow([]);
      for (const note of sheet.notes) ws.addRow([note]);
    }
  }
  const out = await wb.xlsx.writeBuffer();
  // ExcelJS returns a Node Buffer in Node and an ArrayBuffer in the browser: copy into a
  // fresh, exactly sized ArrayBuffer either way.
  return new Uint8Array(out).slice().buffer;
}
