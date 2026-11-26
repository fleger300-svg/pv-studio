import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Stat } from '@pv/ui';
import type { YieldReport } from '../bindings';
import { toCommandError } from '../commands/runCommand';
import { api, unwrap } from '../ipc';
import { useToastStore } from '../store/toast';
import { useViewStore } from '../store/view';

const BALANCE = [
  'horizonKwh',
  'shadingKwh',
  'iamKwh',
  'soilingKwh',
  'lowLightKwh',
  'temperatureKwh',
  'mismatchKwh',
  'wiringMismatchKwh',
  'dcCableKwh',
  'inverterKwh',
  'clippingKwh',
] as const;

/** Annual yield of the project (simulated on demand), monthly bars and energy balance. */
export function ProductionPanel() {
  const { t, i18n } = useTranslation();
  const [report, setReport] = useState<YieldReport | null>(null);
  const [busy, setBusy] = useState(false);
  const fmt = (v: number, digits = 0) =>
    new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits }).format(v);
  const run = async () => {
    setBusy(true);
    try {
      setReport(await unwrap(api.simulateYield()));
    } catch (e) {
      useToastStore.getState().push(toCommandError(e));
    } finally {
      setBusy(false);
    }
  };
  const months = report?.monthlyAcKwh ?? [];
  const max = Math.max(1, ...months);
  return (
    <Panel title={t('production.title')}>
      <Button disabled={busy} onClick={() => void run()}>
        {busy ? t('production.running') : report ? t('production.rerun') : t('production.run')}
      </Button>
      <Button
        variant="secondary"
        onClick={() => {
          useViewStore.getState().setColorMode('shading');
        }}
      >
        {t('production.showShading')}
      </Button>
      {report && (
        <>
          <div className="form-grid">
            <Stat label={t('production.energy')} value={`${fmt(report.acEnergyKwh)} kWh`} />
            <Stat label={t('production.specific')} value={`${fmt(report.specificYield)} kWh/kWc`} />
            <Stat label={t('production.pr')} value={`${fmt(report.performanceRatio * 100, 1)} %`} />
            <Stat label={t('production.poa')} value={`${fmt(report.poaKwhM2)} kWh/m²`} />
          </div>
          <div className="month-bars" role="img" aria-label={t('production.monthly')}>
            {months.map((m, i) => (
              <div key={i} className="month-bars__bar" title={`${fmt(m)} kWh`}>
                <span style={{ height: `${String((m / max) * 100)}%` }} />
                <small>{t(`production.months.${String(i)}`)}</small>
              </div>
            ))}
          </div>
          <table className="library__table">
            <tbody>
              <tr>
                <td>{t('production.balance.nominal')}</td>
                <td>{fmt(report.balance.nominalKwh)} kWh</td>
              </tr>
              {BALANCE.map((k) => (
                <tr key={k}>
                  <td>{t(`production.balance.${k}`)}</td>
                  <td>
                    −{fmt(report.balance[k])} kWh (
                    {fmt((report.balance[k] / report.balance.nominalKwh) * 100, 1)} %)
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="panel-hint">{t('production.note', { source: report.weatherSource })}</p>
        </>
      )}
    </Panel>
  );
}
