/// Excel export worker: ExcelJS is loaded here only, so the main bundle does not grow and
/// the 3D view keeps its frame rate while the file is built.
import ExcelJS from 'exceljs';
import type { WorkbookModel } from '../bom/k2Format';
import { writeWorkbook } from '../bom/xlsx';

export interface BomExcelRequest {
  requestId: number;
  model: WorkbookModel;
}

export type BomExcelResponse =
  | { requestId: number; ok: true; bytes: ArrayBuffer }
  | { requestId: number; ok: false; error: string };

self.onmessage = async (event: MessageEvent<BomExcelRequest>) => {
  const { requestId, model } = event.data;
  try {
    const bytes = await writeWorkbook(ExcelJS, model);
    const reply: BomExcelResponse = { requestId, ok: true, bytes };
    self.postMessage(reply, { transfer: [bytes] });
  } catch (e) {
    const reply: BomExcelResponse = { requestId, ok: false, error: String(e) };
    self.postMessage(reply);
  }
};
