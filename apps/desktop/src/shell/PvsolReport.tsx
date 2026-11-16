import { useTranslation } from 'react-i18next';
import { Button, Panel } from '@pv/ui';
import type { ComparisonRow } from '../bindings';
import { useProjectStore } from '../store/project';

function format(value: number, unit: string, lang: string): string {
  const digits = unit === '%' ? 1 : unit === 'kWp' ? 2 : 0;
  return value.toLocaleString(lang, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
}

/** Gap of PV Studio vs PV*SOL: relative for quantities, in points for percentages. */
function gap(row: ComparisonRow, lang: string): string {
  if (row.unit === '%') {
    const d = row.pvstudio - row.pvsol;
    return `${d >= 0 ? '+' : ''}${format(d, '%', lang)} pt`;
  }
  if (row.pvsol === 0) return '—';
  const d = (row.pvstudio / row.pvsol - 1) * 100;
  return `${d >= 0 ? '+' : ''}${format(d, '%', lang)} %`;
}

/**
 * Report of a PV*SOL import: what was not carried over, and PV Studio's simulation of the
 * imported project next to PV*SOL's own results (same input losses, same weather).
 */
export function PvsolReport() {
  const { t, i18n } = useTranslation();
  const report = useProjectStore((s) => s.pvsolReport);
  const close = useProjectStore((s) => s.closePvsolReport);
  if (!report) return null;
  const lang = i18n.language;
  const c = report.comparison;

  return (
    <div className="pvsol-report" role="dialog" aria-modal="true" aria-label={t('pvsol.title')}>
      <Panel title={t('pvsol.title')}>
        <p className="panel-hint">{t('pvsol.source', { file: report.source })}</p>

        <h3 className="pvsol-report__heading">{t('pvsol.notesTitle')}</h3>
        {report.notes.length === 0 ? (
          <p>{t('pvsol.complete')}</p>
        ) : (
          <ul className="pvsol-report__notes">
            {report.notes.map((n, i) => (
              <li key={i}>{t(n.key, n.params)}</li>
            ))}
          </ul>
        )}

        <h3 className="pvsol-report__heading">{t('pvsol.compareTitle')}</h3>
        {c ? (
          <>
            <p className="panel-hint">
              {t(c.sameWeather ? 'pvsol.sameWeather' : 'pvsol.otherWeather')}
            </p>
            <table className="pvsol-report__table">
              <thead>
                <tr>
                  <th />
                  <th>PV*SOL</th>
                  <th>PV Studio</th>
                  <th>{t('pvsol.gap')}</th>
                </tr>
              </thead>
              <tbody>
                {c.rows.map((r) => (
                  <tr key={r.key}>
                    <th scope="row">
                      {t(`pvsol.compare.${r.key}`)} <span className="panel-hint">{r.unit}</span>
                    </th>
                    <td>{format(r.pvsol, r.unit, lang)}</td>
                    <td>{format(r.pvstudio, r.unit, lang)}</td>
                    <td>{gap(r, lang)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="panel-hint">
              {t('pvsol.assumptions', {
                soiling: format(c.assumptions.soilingLoss * 100, '%', lang),
                mismatch: format(c.assumptions.mismatchLoss * 100, '%', lang),
                cables: format(c.assumptions.dcCableLoss * 100, '%', lang),
              })}
            </p>
            <p className="panel-hint">{t('pvsol.notModelled')}</p>
          </>
        ) : (
          <p>{t('pvsol.noComparison')}</p>
        )}

        <div className="library__actions">
          <Button variant="primary" onClick={close}>
            {t('pvsol.close')}
          </Button>
        </div>
      </Panel>
    </div>
  );
}
