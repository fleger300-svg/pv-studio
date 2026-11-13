import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { create } from 'zustand';
import { Button } from '@pv/ui';
import type {
  BomReport_Serialize,
  FieldReport_Serialize,
  WiringReport_Serialize,
  YieldReport,
} from '../bindings';
import { api, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';

interface ReportState {
  open: boolean;
  setOpen: (open: boolean) => void;
}

export const useReportStore = create<ReportState>()((set) => ({
  open: false,
  setOpen: (open) => {
    set({ open });
  },
}));

/**
 * Project report (A4, printable; « Imprimer → Enregistrer en PDF »): location and weather,
 * buildings and fields, statics per field, bill of materials (quantities), inverters and
 * string plan (Voc × kT), yield and energy balance. Built from the same engines as the
 * screens: the report always matches the model.
 */
export function ReportView() {
  const { t, i18n } = useTranslation();
  const open = useReportStore((s) => s.open);
  const setOpen = useReportStore((s) => s.setOpen);
  const summary = useProjectStore((s) => s.summary);
  const [statics, setStatics] = useState<FieldReport_Serialize[]>([]);
  const [bom, setBom] = useState<BomReport_Serialize | null>(null);
  const [wiring, setWiring] = useState<WiringReport_Serialize | null>(null);
  const [yieldReport, setYield] = useState<YieldReport | null>(null);

  useEffect(() => {
    if (!open) return;
    void unwrap(api.getStaticsSnapshot()).then((s) => {
      setStatics(s.fields);
    });
    void unwrap(api.getBom()).then(setBom);
    void unwrap(api.wiringReport()).then(setWiring);
    unwrap(api.simulateYield())
      .then(setYield)
      .catch(() => {
        setYield(null);
      });
  }, [open]);

  if (!open || !summary) return null;
  const n = (v: number, d = 1) =>
    new Intl.NumberFormat(i18n.language, { maximumFractionDigits: d }).format(v);
  const faceName = (id: string) => summary.roofFaces.find((f) => f.face.id === id)?.face.name ?? '';
  const total = bom?.outcome.status === 'ok' ? bom.outcome.bom.total : null;

  return (
    <div className="report-overlay">
      <div className="report-toolbar">
        <Button
          onClick={() => {
            window.print();
          }}
        >
          {t('report.print')}
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            setOpen(false);
          }}
        >
          {t('report.close')}
        </Button>
      </div>
      <article className="report">
        <h1>{summary.name}</h1>
        <p className="report__meta">
          {t('report.date', { date: new Date().toLocaleDateString(i18n.language) })}
        </p>

        <h2>{t('report.site')}</h2>
        <table>
          <tbody>
            <tr>
              <th>{t('location.address')}</th>
              <td>{summary.location?.label ?? '—'}</td>
            </tr>
            <tr>
              <th>{t('location.altitude')}</th>
              <td>{summary.location ? `${n(summary.location.altitudeM, 0)} m` : '—'}</td>
            </tr>
            <tr>
              <th>{t('location.irradiation')}</th>
              <td>
                {summary.weather
                  ? `${n(summary.weather.annualGhiKwhM2, 0)} kWh/m² (${summary.weather.source})`
                  : '—'}
              </td>
            </tr>
          </tbody>
        </table>

        <h2>{t('report.fields')}</h2>
        <table>
          <thead>
            <tr>
              <th>{t('report.face')}</th>
              <th>{t('report.module')}</th>
              <th>{t('report.mounting')}</th>
              <th>{t('arrays.modules')}</th>
              <th>kWc</th>
            </tr>
          </thead>
          <tbody>
            {summary.arrays.map((a) => (
              <tr key={a.array.id}>
                <td>{faceName(a.array.roofFaceId)}</td>
                <td>{a.moduleLabel ?? '—'}</td>
                <td>{a.mountingLabel ?? '—'}</td>
                <td>{a.moduleCount}</td>
                <td>{n((a.moduleCount * summary.modulePeakPowerWp) / 1000, 2)}</td>
              </tr>
            ))}
          </tbody>
        </table>

        <h2>{t('report.statics')}</h2>
        <table>
          <tbody>
            {statics.map((f) => {
              const a = summary.arrays.find((x) => x.array.id === f.arrayId);
              return (
                <tr key={f.arrayId}>
                  <th>{a ? faceName(a.array.roofFaceId) : ''}</th>
                  <td>
                    {f.outcome.status === 'ok'
                      ? `${f.outcome.result.ok ? t('report.ok') : t('report.notOk')} · η max ${n(
                          (f.outcome.result.maxUtilisation ?? 0) * 100,
                          0,
                        )} %${
                          f.outcome.result.ballastBlocks
                            ? ` · ${t('report.ballast', { count: f.outcome.result.ballastBlocks })}`
                            : ''
                        }`
                      : t(f.outcome.error.key, f.outcome.error.params)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <p className="report__note">{t('report.staticsNote')}</p>

        {total && (
          <>
            <h2>{t('report.bom')}</h2>
            <table>
              <thead>
                <tr>
                  <th>{t('report.article')}</th>
                  <th>{t('report.label')}</th>
                  <th>{t('report.quantity')}</th>
                  <th>{t('report.mass')}</th>
                </tr>
              </thead>
              <tbody>
                {total.lines.map((l) => (
                  <tr key={l.articleNo + l.label}>
                    <td>{l.articleNo}</td>
                    <td>{l.descriptions[i18n.language] ?? l.label}</td>
                    <td>{l.quantity}</td>
                    <td>{n(l.massKg)} kg</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}

        {wiring && wiring.inverters.length > 0 && (
          <>
            <h2>{t('report.wiring')}</h2>
            {wiring.inverters.map((inv) => (
              <div key={inv.id}>
                <h3>
                  {inv.name} · {inv.model} · {t('connection.ratio')} {n(inv.dcAcRatio * 100)} %
                </h3>
                <table>
                  <thead>
                    <tr>
                      <th>{t('connection.tracker')}</th>
                      <th>{t('connection.strings')}</th>
                      <th>Voc STC</th>
                      <th>{t('connection.vocPlan')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {inv.trackers.flatMap((tr) =>
                      tr.strings.map((s, k) => (
                        <tr key={s.id}>
                          <td>{k === 0 ? tr.index + 1 : ''}</td>
                          <td>{t('connection.stringModules', { count: s.modules })}</td>
                          <td>{n(s.vocStcV)} V</td>
                          <td>{n(s.vocPlanV)} V</td>
                        </tr>
                      )),
                    )}
                  </tbody>
                </table>
              </div>
            ))}
          </>
        )}

        {yieldReport && (
          <>
            <h2>{t('production.title')}</h2>
            <table>
              <tbody>
                <tr>
                  <th>{t('production.energy')}</th>
                  <td>{n(yieldReport.acEnergyKwh, 0)} kWh</td>
                </tr>
                <tr>
                  <th>{t('production.specific')}</th>
                  <td>{n(yieldReport.specificYield, 0)} kWh/kWc</td>
                </tr>
                <tr>
                  <th>{t('production.pr')}</th>
                  <td>{n(yieldReport.performanceRatio * 100)} %</td>
                </tr>
              </tbody>
            </table>
            <p className="report__note">
              {t('production.note', { source: yieldReport.weatherSource })}
            </p>
          </>
        )}
      </article>
    </div>
  );
}
