import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Select } from '@pv/ui';
import type { Currency, ImportReport, ImportRequest, ImportSheet } from '../bindings';
import { api, unwrap } from '../ipc';
import { toCommandError } from '../commands/runCommand';
import { useLibraryStore } from '../store/library';
import { useToastStore } from '../store/toast';
import type { PriceListResponse } from '../workers/priceList.worker';
import { TextInput } from './inputs';
import { base64ToBytes } from './priceList';
import { today } from './LibraryView';

let worker: Worker | null = null;
let nextId = 1;

/** Reads the workbook's sheets in the price list worker (created on first use). */
function readInWorker(bytes: ArrayBuffer): Promise<ImportSheet[]> {
  worker ??= new Worker(new URL('../workers/priceList.worker.ts', import.meta.url), {
    type: 'module',
  });
  const w = worker;
  const requestId = nextId++;
  return new Promise((resolve, reject) => {
    const onMessage = (e: MessageEvent<PriceListResponse>) => {
      if (e.data.requestId !== requestId) return;
      w.removeEventListener('message', onMessage);
      if (e.data.ok) resolve(e.data.sheets);
      else reject(new Error(e.data.error));
    };
    w.addEventListener('message', onMessage);
    w.postMessage({ requestId, bytes }, [bytes]);
  });
}

/**
 * Price list import: choose the file, check what will happen (preview), import. Prices are
 * added as new periods; differences with the library are listed, never applied.
 */
export function ImportDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const changed = useLibraryStore((s) => s.changed);
  const [file, setFile] = useState<{ name: string; sheets: ImportSheet[] } | null>(null);
  const [manufacturer, setManufacturer] = useState('K2 Systems');
  const [supplier, setSupplier] = useState('K2 Systems');
  const [currency, setCurrency] = useState<Currency>('CHF');
  const [from, setFrom] = useState(today());
  const [createMissing, setCreateMissing] = useState(false);
  const [preview, setPreview] = useState<ImportReport | null>(null);
  const [done, setDone] = useState<ImportReport | null>(null);
  const [busy, setBusy] = useState(false);

  const request = (): ImportRequest | null =>
    file && {
      manufacturer,
      supplier,
      currency,
      validFrom: from,
      createMissing,
      sheets: file.sheets,
    };

  const choose = async (path: string | null = null) => {
    setBusy(true);
    try {
      const picked = await unwrap(api.libraryPickFile('priceList', path));
      if (!picked) return;
      const bytes = base64ToBytes(picked.base64).buffer;
      setFile({ name: picked.name, sheets: await readInWorker(bytes) });
      setDone(null);
    } catch (e) {
      useToastStore.getState().push(toCommandError(e));
    } finally {
      setBusy(false);
    }
  };

  // Development builds: automated checks choose the file without the native dialog.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    Object.assign(window, { __pvImportFile: (path: string) => choose(path) });
  });

  // The preview follows every setting.
  useEffect(() => {
    const req = request();
    if (!req) return;
    let live = true;
    unwrap(api.libraryImport(req, false))
      .then((r) => {
        if (live) setPreview(r);
      })
      .catch((e: unknown) => {
        useToastStore.getState().push(toCommandError(e));
      });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [file, manufacturer, supplier, currency, from, createMissing]);

  const run = async () => {
    const req = request();
    if (!req) return;
    setBusy(true);
    try {
      setDone(await unwrap(api.libraryImport(req, true)));
      changed();
    } catch (e) {
      useToastStore.getState().push(toCommandError(e));
    } finally {
      setBusy(false);
    }
  };

  const shown = done ?? preview;
  return (
    <div
      className="library__dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t('library.import.title')}
    >
      <Panel title={t('library.import.title')}>
        <div className="library__actions">
          <Button variant="secondary" disabled={busy} onClick={() => void choose()}>
            {t('library.import.choose')}
          </Button>
          <span className="panel-hint">{file ? file.name : t('library.import.noFile')}</span>
        </div>
        <div className="form-grid">
          <TextInput
            label={t('library.fields.manufacturer')}
            value={manufacturer}
            onChange={setManufacturer}
          />
          <TextInput label={t('library.prices.supplier')} value={supplier} onChange={setSupplier} />
          <Select<Currency>
            label={t('library.prices.currency')}
            value={currency}
            options={(['CHF', 'EUR'] as const).map((c) => ({ value: c, label: c }))}
            onChange={setCurrency}
          />
          <label className="pv-field form-field">
            <span className="pv-field__label">{t('library.prices.from')}</span>
            <input
              type="date"
              className="form-input"
              value={from}
              onChange={(e) => {
                setFrom(e.target.value);
              }}
            />
          </label>
        </div>
        <label className="form-check">
          <input
            type="checkbox"
            checked={createMissing}
            onChange={(e) => {
              setCreateMissing(e.target.checked);
            }}
          />
          {t('library.import.createMissing')}
        </label>
        {shown && (
          <div className="library__report" role="status">
            <p>
              {done
                ? t('library.import.done', { count: shown.priced.length })
                : t('library.import.preview', { count: shown.priced.length })}
            </p>
            {shown.created.length > 0 && (
              <p>{t('library.import.created', { list: shown.created.join(', ') })}</p>
            )}
            {shown.unknown.length > 0 && (
              <p>{t('library.import.unknown', { list: shown.unknown.join(', ') })}</p>
            )}
            {shown.conflicts.length > 0 && (
              <p>{t('library.import.conflicts', { list: shown.conflicts.join(', ') })}</p>
            )}
            {shown.differences.map((d) => (
              <p key={`${d.articleNo}-${d.field}`}>
                {t('library.import.difference', {
                  article: d.articleNo,
                  field: t(`library.import.fields.${d.field}`),
                  library: d.library,
                  list: d.list,
                })}
              </p>
            ))}
          </div>
        )}
        <div className="library__actions">
          <Button
            disabled={!file || busy || done !== null || (preview?.priced.length ?? 0) === 0}
            onClick={() => void run()}
          >
            {t('library.import.run')}
          </Button>
          <Button variant="secondary" onClick={onClose}>
            {t('library.close')}
          </Button>
        </div>
      </Panel>
    </div>
  );
}
