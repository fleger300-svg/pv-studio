import { useTranslation } from 'react-i18next';
import { Panel, Stat } from '@pv/ui';
import type { FieldStaticsResult_Serialize } from '../bindings';
import { useProjectStore } from '../store/project';
import { useStaticsStore, type FieldReport } from '../store/statics';

type Status = 'ok' | 'over' | 'failed' | 'pending';

function statusOf(report: FieldReport | undefined): Status {
  if (!report) return 'pending';
  if (report.outcome.status === 'failed') return 'failed';
  return report.outcome.result.ok ? 'ok' : 'over';
}

function FieldResult({ result }: { result: FieldStaticsResult_Serialize }) {
  const { t, i18n } = useTranslation();
  const fmt = (x: number, digits: number) =>
    new Intl.NumberFormat(i18n.language, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(x);
  const spacings = result.decisions.map((d) => d.fixationSpacingM);
  const lo = Math.min(...spacings);
  const hi = Math.max(...spacings);
  return (
    <>
      {result.maxUtilisation !== null && (
        <Stat
          label={t('statics.maxUtilisation')}
          value={t('statics.percent', { value: fmt(result.maxUtilisation * 100, 0) })}
        />
      )}
      {spacings.length > 0 && (
        <Stat
          label={t('statics.spacing')}
          value={t('statics.metres', {
            value: lo === hi ? fmt(lo, 2) : `${fmt(lo, 2)}–${fmt(hi, 2)}`,
          })}
        />
      )}
      {result.fixations !== null && (
        <Stat label={t('statics.fixations')} value={fmt(result.fixations, 0)} />
      )}
      {result.climbers !== null && (
        <Stat label={t('statics.climbers')} value={fmt(result.climbers, 0)} />
      )}
      {result.ballastBlocks !== null && (
        <Stat label={t('statics.ballastBlocks')} value={fmt(result.ballastBlocks, 0)} />
      )}
      <Stat label={t('statics.zones')} value={String(result.zones.length)} />
    </>
  );
}

/** Read-only statics of every field (computed in Rust, updated after each command). */
export function StaticsPanel() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const fields = useStaticsStore((s) => s.fields);
  const busy = useStaticsStore((s) => s.busy);
  const failure = useStaticsStore((s) => s.failure);
  if (!summary) return null;

  return (
    <Panel title={t('statics.title')}>
      <p className="statics__state" role="status" aria-live="polite">
        {busy ? t('statics.computing') : t('statics.upToDate')}
      </p>
      {failure && <p className="statics__error">{t(failure.key, failure.params)}</p>}
      <ul className="statics__list">
        {summary.arrays.map((a, i) => {
          const report = fields[a.array.id];
          const status = statusOf(report);
          const face = summary.roofFaces[a.roofFaceIndex]?.face.name ?? '';
          return (
            <li key={a.array.id} className="statics__field">
              <div className="statics__head">
                <span>{t('statics.field', { index: i + 1, face })}</span>
                <span className={`statics__badge statics__badge--${status}`}>
                  {t(`statics.status.${status}`)}
                </span>
              </div>
              {report?.outcome.status === 'ok' && <FieldResult result={report.outcome.result} />}
              {report?.outcome.status === 'failed' && (
                <p className="statics__error">
                  {t(report.outcome.error.key, report.outcome.error.params)}
                </p>
              )}
            </li>
          );
        })}
      </ul>
      <p className="statics__disclaimer">{t('statics.disclaimer')}</p>
    </Panel>
  );
}
