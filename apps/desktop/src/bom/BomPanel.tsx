import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Stat } from '@pv/ui';
import type { CommandError } from '../bindings';
import { api, BackendError, unwrap } from '../ipc';
import { useBomStore } from '../store/bom';
import { useProjectStore } from '../store/project';
import { useToastStore } from '../store/toast';
import { BomTableView } from './BomTableView';
import { exportBom } from './exportBom';

function toCommandError(e: unknown): CommandError {
  if (e instanceof BackendError) return { key: e.key, params: e.params };
  return { key: 'errors.store.failure', params: { detail: String(e) } };
}

/** Discount of one manufacturer, in %, applied on Enter or when the field loses focus. */
function DiscountField({ manufacturer, rate }: { manufacturer: string; rate: number }) {
  const { t } = useTranslation();
  const shown = String(Math.round(rate * 1000) / 10);
  // The field is keyed on the rate (see BomPanel): a new rate remounts it with a fresh draft.
  const [draft, setDraft] = useState(shown);

  const commit = async () => {
    const value = draft.trim() === '' ? 0 : Number(draft.replace(',', '.'));
    const next = value / 100;
    if (Number.isNaN(next) || next === rate) {
      setDraft(shown);
      return;
    }
    try {
      // A command like any other: journalled, undoable (Ctrl+Z).
      await unwrap(
        api.applyCommand({
          type: 'SetDiscount',
          manufacturer,
          rate: next === 0 ? null : next,
        }),
      );
    } catch (e) {
      setDraft(shown);
      useToastStore.getState().push(toCommandError(e));
    }
  };

  return (
    <label className="bom-panel__discount">
      <span>{t('bom.discount', { manufacturer })}</span>
      <input
        className="bom-panel__input"
        inputMode="decimal"
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onBlur={() => void commit()}
        onKeyDown={(e) => {
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          if (e.key === 'Escape') setDraft(shown);
        }}
      />
      <span>%</span>
    </label>
  );
}

/** Totals of the bill of materials, discounts, and its full view / Excel export. */
export function BomPanel() {
  const { t, i18n } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const report = useBomStore((s) => s.report);
  const [open, setOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const projectName = summary?.name ?? 'PV Studio';

  // Development builds: lets automated checks export without the native dialog.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const hook = (path: string) => exportBom(projectName, i18n.language, t, path);
    Object.assign(window, { __pvExportBom: hook });
  }, [projectName, i18n.language, t]);

  if (!summary) return null;
  const money = (x: number) =>
    new Intl.NumberFormat(i18n.language, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(x);
  const n = (x: number, d = 0) =>
    new Intl.NumberFormat(i18n.language, { maximumFractionDigits: d }).format(x);

  const run = async () => {
    setExporting(true);
    setSaved(null);
    try {
      const r = await exportBom(projectName, i18n.language, t);
      setSaved(r.path);
    } catch (e) {
      useToastStore.getState().push(toCommandError(e));
    } finally {
      setExporting(false);
    }
  };

  let body;
  if (!report) {
    body = <p className="statics__state">{t('bom.pending')}</p>;
  } else if (report.outcome.status === 'failed') {
    const { error } = report.outcome;
    body = <p className="statics__error">{t(error.key, error.params)}</p>;
  } else if (report.outcome.bom.total.lines.length === 0) {
    body = <p className="statics__state">{t('bom.empty')}</p>;
  } else {
    const bom = report.outcome.bom;
    const lines = bom.total.lines;
    const manufacturers = [...new Map(lines.map((l) => [l.manufacturer, l.discountRate]))];
    body = (
      <>
        <Stat
          label={t('bom.price')}
          value={
            // Presets carry no prices (they come from the component library): say so.
            lines.every((l) => l.listPrice === 0)
              ? t('bom.noPrices')
              : `${money(bom.total.price)} ${t('bom.currency')}`
          }
        />
        <Stat label={t('bom.mass')} value={t('bom.kg', { value: n(bom.total.massKg, 1) })} />
        <Stat label={t('bom.saleUnits')} value={n(lines.reduce((s, l) => s + l.saleUnits, 0))} />
        {bom.savedBars.length > 0 && (
          <Stat
            label={t('bom.savedBars')}
            value={n(bom.savedBars.reduce((s, b) => s + b.perFaces - b.project, 0))}
          />
        )}
        {manufacturers.map(([m, rate]) => (
          <DiscountField key={`${m}:${String(rate)}`} manufacturer={m} rate={rate} />
        ))}
        <div className="bom-panel__actions">
          <Button
            onClick={() => {
              setOpen(true);
            }}
          >
            {t('bom.view')}
          </Button>
          <Button variant="primary" disabled={exporting} onClick={() => void run()}>
            {exporting ? t('bom.exporting') : t('bom.export')}
          </Button>
        </div>
        {saved && <p className="statics__state">{t('bom.exported', { path: saved })}</p>}
        {open && (
          <BomTableView
            bom={bom}
            onClose={() => {
              setOpen(false);
            }}
          />
        )}
      </>
    );
  }
  return <Panel title={t('bom.title')}>{body}</Panel>;
}
