/// Price list reader worker: ExcelJS is loaded here only (as for the Excel export), so the
/// main bundle does not grow and the UI stays responsive on large lists.
import ExcelJS from 'exceljs';
import type { ImportSheet } from '../bindings';
import { readPriceList } from '../library/priceList';

export interface PriceListRequest {
  requestId: number;
  bytes: ArrayBuffer;
}

export type PriceListResponse =
  | { requestId: number; ok: true; sheets: ImportSheet[] }
  | { requestId: number; ok: false; error: string };

self.onmessage = async (event: MessageEvent<PriceListRequest>) => {
  const { requestId, bytes } = event.data;
  try {
    const sheets = await readPriceList(ExcelJS, bytes);
    const reply: PriceListResponse = { requestId, ok: true, sheets };
    self.postMessage(reply);
  } catch (e) {
    const reply: PriceListResponse = { requestId, ok: false, error: String(e) };
    self.postMessage(reply);
  }
};
