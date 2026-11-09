import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { IconButton } from '@pv/ui';
import type { Bom_Serialize } from '../bindings';
import { COLUMNS, description } from './k2Format';

interface Props {
  bom: Bom_Serialize;
  onClose: () => void;
}

/** Full-width view of the bill of materials: Total and one tab per face, K2 columns. */
export function BomTableView({ bom, onClose }: Props) {
  const { t, i18n } = useTranslation();
  const tables = [bom.total, ...bom.faces];
  const [tab, setTab] = useState(0);
  const table = tables[Math.min(tab, tables.length - 1)] ?? bom.total;
  const lang = i18n.language;
  const num = (x: number, digits: number) =>
    new Intl.NumberFormat(lang, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(x);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  return (
    <div className="bom-overlay" role="dialog" aria-modal="true" aria-label={t('bom.title')}>
      <div className="bom-overlay__panel">
        <div className="bom-overlay__header">
          <h2>{t('bom.title')}</h2>
          <IconButton label={t('bom.close')} icon="×" onClick={onClose} />
        </div>
        <div className="bom-overlay__tabs" role="tablist">
          {tables.map((tb, i) => (
            <button
              key={tb.faceId ?? 'total'}
              type="button"
              role="tab"
              aria-selected={i === tab}
              className="bom-overlay__tab"
              onClick={() => {
                setTab(i);
              }}
            >
              {i === 0 ? t('bom.total') : tb.title}
            </button>
          ))}
        </div>
        <div className="bom-overlay__scroll" role="tabpanel">
          <table className="bom-table">
            <thead>
              <tr>
                {COLUMNS.map((c) => (
                  <th key={c}>{t(`bom.columns.${c}`)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.lines.map((l) => (
                <tr key={l.articleNo}>
                  <td>{l.articleNo}</td>
                  <td>{description(l, lang)}</td>
                  <td>{num(l.quantity, 0)}</td>
                  <td>{num(l.massKg, 3)}</td>
                  <td>{num(l.saleUnits, 0)}</td>
                  <td>{num(l.packQty, 0)}</td>
                  <td>{num(l.listPrice, 2)}</td>
                  <td>{t('bom.percent', { value: num(l.discountRate * 100, 0) })}</td>
                  <td>{num(l.unitPrice, 2)}</td>
                  <td>{num(l.price, 2)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <td colSpan={3}>{t('bom.total')}</td>
                <td>{num(table.massKg, 3)}</td>
                <td colSpan={5} />
                <td>{num(table.price, 2)}</td>
              </tr>
            </tfoot>
          </table>
          {tab === 0 &&
            bom.savedBars.map((s) => (
              <p key={s.articleNo} className="bom-overlay__note">
                {t('bom.savedBarsNote', {
                  articleNo: s.articleNo,
                  perFaces: s.perFaces,
                  project: s.project,
                  saved: s.perFaces - s.project,
                })}
              </p>
            ))}
        </div>
      </div>
    </div>
  );
}
