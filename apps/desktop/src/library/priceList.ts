/// Price lists: the workbook is only read here (raw cell values, every sheet); recognising
/// columns, matching articles and computing prices is done in Rust (`pv_library::import`).
import type ExcelJSNamespace from 'exceljs';
import type { ImportCell, ImportSheet } from '../bindings';

type ExcelJS = typeof ExcelJSNamespace;

function cell(value: unknown): ImportCell {
  if (value === null || value === undefined || value === '') return { kind: 'empty' };
  if (typeof value === 'number') return { kind: 'number', value };
  if (typeof value === 'string') return { kind: 'text', value };
  if (typeof value === 'boolean') return { kind: 'text', value: String(value) };
  if (value instanceof Date) return { kind: 'text', value: value.toISOString().slice(0, 10) };
  if (typeof value === 'object') {
    const v = value as {
      result?: unknown;
      richText?: { text: string }[];
      text?: string;
    };
    // Formulas: their cached result.
    if ('result' in v) return cell(v.result);
    if (v.richText) return { kind: 'text', value: v.richText.map((t) => t.text).join('') };
    if (typeof v.text === 'string') return { kind: 'text', value: v.text };
  }
  return { kind: 'empty' };
}

/** Raw rows of every sheet of an `.xlsx` file. */
export async function readPriceList(excel: ExcelJS, bytes: ArrayBuffer): Promise<ImportSheet[]> {
  const workbook = new excel.Workbook();
  await workbook.xlsx.load(bytes);
  return workbook.worksheets.map((ws) => {
    const rows: ImportCell[][] = [];
    ws.eachRow({ includeEmpty: false }, (row) => {
      const values = Array.isArray(row.values) ? row.values.slice(1) : [];
      rows.push(values.map(cell));
    });
    return { name: ws.name, rows };
  });
}

/** Bytes of a base64 string (files read by Rust). */
export function base64ToBytes(base64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
