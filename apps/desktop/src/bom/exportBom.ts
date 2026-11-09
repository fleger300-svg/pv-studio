import { api, unwrap } from '../ipc';
import type { BomExcelRequest, BomExcelResponse } from '../workers/bomExcel.worker';
import { buildWorkbookModel, type Translate, type WorkbookModel } from './k2Format';

let worker: Worker | null = null;
let nextId = 1;

/** Builds the `.xlsx` bytes in the export worker (created on first use). */
function buildInWorker(model: WorkbookModel): Promise<ArrayBuffer> {
  worker ??= new Worker(new URL('../workers/bomExcel.worker.ts', import.meta.url), {
    type: 'module',
  });
  const w = worker;
  const requestId = nextId++;
  return new Promise((resolve, reject) => {
    const onMessage = (e: MessageEvent<BomExcelResponse>) => {
      if (e.data.requestId !== requestId) return;
      w.removeEventListener('message', onMessage);
      if (e.data.ok) resolve(e.data.bytes);
      else reject(new Error(e.data.error));
    };
    w.addEventListener('message', onMessage);
    const request: BomExcelRequest = { requestId, model };
    w.postMessage(request);
  });
}

export interface ExportResult {
  /** Saved file, or `null` if the user cancelled the dialog. */
  path: string | null;
  snapshotId: number;
}

/**
 * Exports the current bill of materials to Excel, K2 layout: it is frozen first (the file is
 * always a recorded snapshot), read back, built in the worker, then saved where the user
 * chooses (`path` skips the dialog: automation).
 */
export async function exportBom(
  projectName: string,
  lang: string,
  t: Translate,
  path: string | null = null,
): Promise<ExportResult> {
  const stamp = new Date().toISOString().slice(0, 16).replace('T', ' ');
  const info = await unwrap(api.freezeBom(t('bom.snapshotLabel', { date: stamp })));
  const frozen = await unwrap(api.getBomSnapshot(info.id));
  const bytes = await buildInWorker(buildWorkbookModel(frozen.bom, lang, t));
  const name = `${projectName} - ${t('bom.fileName')} ${String(info.id)}.xlsx`;
  const saved = await unwrap(api.saveExport(name, Array.from(new Uint8Array(bytes)), path));
  return { path: saved, snapshotId: info.id };
}
