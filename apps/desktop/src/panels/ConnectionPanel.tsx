import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Stat } from '@pv/ui';
import type {
  ComponentSummary_Serialize,
  Issue,
  InverterReport,
  Wiring_Serialize,
  WiringSettings,
} from '../bindings';
import { runApplied, runCommand, toCommandError } from '../commands/runCommand';
import { NumberField } from '../forms/fields';
import { api, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useToastStore } from '../store/toast';
import { useViewStore } from '../store/view';
import { useWiringStore } from '../store/wiring';

const DEFAULT_SETTINGS: WiringSettings = {
  minCellTempC: -10,
  maxCellTempC: 70,
  dcAcMin: 0.9,
  dcAcMax: 1.3,
  vocFactor: 1.15,
};

function report(e: unknown) {
  useToastStore.getState().push(toCommandError(e));
}

/** Replaces the project's wiring (one undo step; inverters are frozen from the library). */
async function setWiring(wiring: Wiring_Serialize): Promise<boolean> {
  return (await runCommand('apply', { type: 'SetWiring', wiring })) !== null;
}

/**
 * Connection step: inverters, strings by tracker with their checks (as PV*SOL), automatic
 * stringing, design temperatures. The 3D view colours the modules by string.
 */
export function ConnectionPanel() {
  const { t, i18n } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const wiringReport = useWiringStore((s) => s.report);
  const refresh = useWiringStore((s) => s.refresh);
  const setColorMode = useViewStore((s) => s.setColorMode);

  useEffect(() => {
    setColorMode('string');
  }, [setColorMode]);
  useEffect(() => {
    if (summary) refresh().catch(report);
  }, [summary, refresh]);

  if (!summary) return null;
  // The summary type also covers its deserialised form (optional defaults): normalise.
  const raw = summary.wiring;
  const wiring: Wiring_Serialize = {
    inverters: raw.inverters,
    strings: raw.strings.map((x) => ({ ...x, locked: x.locked ?? false })),
    settings: raw.settings ?? DEFAULT_SETTINGS,
  };
  const fmt = (v: number, digits = 1) =>
    new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits }).format(v);

  return (
    <>
      <Panel title={t('connection.title')}>
        {wiringReport && (
          <div className="form-grid">
            <Stat
              label={t('connection.dcPower')}
              value={`${fmt(wiringReport.dcPowerWp / 1000, 2)} kWc`}
            />
            <Stat
              label={t('connection.acPower')}
              value={`${fmt(wiringReport.acPowerW / 1000, 2)} kW`}
            />
            <Stat
              label={t('connection.assigned')}
              value={`${String(wiringReport.activeModules - wiringReport.unassignedModules)} / ${String(wiringReport.activeModules)}`}
            />
            <Stat
              label={t('connection.status')}
              value={t(`connection.severity.${wiringReport.worst ?? 'ok'}`)}
            />
          </div>
        )}
        {wiringReport && <IssueList issues={wiringReport.issues} />}
        <div className="library__actions">
          <Button
            disabled={wiring.inverters.length === 0}
            onClick={() => void runApplied('apply', () => unwrap(api.wiringAuto()))}
          >
            {t('connection.auto')}
          </Button>
          <Button
            variant="secondary"
            disabled={!wiring.strings.some((s) => !s.locked)}
            onClick={() =>
              void setWiring({ ...wiring, strings: wiring.strings.filter((s) => s.locked) })
            }
          >
            {t('connection.clearStrings')}
          </Button>
        </div>
        {wiring.inverters.length === 0 && (
          <p className="panel-hint">{t('connection.noInverter')}</p>
        )}
      </Panel>
      {wiringReport?.inverters.map((inv) => (
        <InverterCard key={inv.id} inv={inv} wiring={wiring} fmt={fmt} />
      ))}
      <AddInverter wiring={wiring} />
      <SettingsForm wiring={wiring} />
    </>
  );
}

function IssueList({ issues }: { issues: Issue[] }) {
  const { t } = useTranslation();
  if (issues.length === 0) return null;
  return (
    <ul className="issue-list">
      {issues.map((i, k) => (
        <li key={`${i.key}-${String(k)}`} className={`issue issue--${i.severity}`}>
          {t(i.key, i.params)}
        </li>
      ))}
    </ul>
  );
}

function InverterCard({
  inv,
  wiring,
  fmt,
}: {
  inv: InverterReport;
  wiring: Wiring_Serialize;
  fmt: (v: number, digits?: number) => string;
}) {
  const { t } = useTranslation();
  const remove = () =>
    setWiring({
      ...wiring,
      inverters: wiring.inverters.filter((i) => i.id !== inv.id),
      strings: wiring.strings.filter((s) => s.inverter !== inv.id),
    });
  const toggleLock = (id: string) =>
    setWiring({
      ...wiring,
      strings: wiring.strings.map((s) => (s.id === id ? { ...s, locked: !s.locked } : s)),
    });
  return (
    <Panel title={`${inv.name} · ${inv.model}`}>
      <div className="form-grid">
        <Stat label={t('connection.acPower')} value={`${fmt(inv.acPowerW / 1000, 2)} kW`} />
        <Stat label={t('connection.dcPower')} value={`${fmt(inv.dcPowerWp / 1000, 2)} kWc`} />
        <Stat label={t('connection.ratio')} value={`${fmt(inv.dcAcRatio * 100)} %`} />
      </div>
      <IssueList issues={inv.issues} />
      <table className="library__table">
        <thead>
          <tr>
            <th>{t('connection.tracker')}</th>
            <th>{t('connection.strings')}</th>
            <th>{t('connection.vocPlan')}</th>
            <th>{t('connection.vocCold')}</th>
            <th>{t('connection.isc')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {inv.trackers.flatMap((tr) =>
            tr.strings.length === 0
              ? [
                  <tr key={`${String(tr.index)}-empty`}>
                    <td>{tr.index + 1}</td>
                    <td colSpan={5} className="panel-hint">
                      {t('connection.emptyTracker')}
                    </td>
                  </tr>,
                ]
              : tr.strings.map((s, k) => {
                  const locked = wiring.strings.find((x) => x.id === s.id)?.locked ?? false;
                  return (
                    <tr key={s.id}>
                      <td>{k === 0 ? tr.index + 1 : ''}</td>
                      <td>{t('connection.stringModules', { count: s.modules })}</td>
                      <td>{fmt(s.vocPlanV, 0)} V</td>
                      <td>{fmt(s.vocColdV, 0)} V</td>
                      <td>{fmt(s.iscA, 2)} A</td>
                      <td>
                        <button
                          type="button"
                          className="link-button"
                          aria-pressed={locked}
                          title={t('connection.lockHint')}
                          onClick={() => void toggleLock(s.id)}
                        >
                          {locked ? t('connection.locked') : t('connection.unlocked')}
                        </button>
                      </td>
                    </tr>
                  );
                }),
          )}
        </tbody>
      </table>
      {inv.trackers.map((tr) => (
        <IssueList key={tr.index} issues={tr.issues.concat(tr.strings.flatMap((s) => s.issues))} />
      ))}
      <Button variant="secondary" onClick={() => void remove()}>
        {t('connection.removeInverter')}
      </Button>
    </Panel>
  );
}

function AddInverter({ wiring }: { wiring: Wiring_Serialize }) {
  const { t } = useTranslation();
  const [text, setText] = useState('');
  const [rows, setRows] = useState<ComponentSummary_Serialize[]>([]);
  useEffect(() => {
    let live = true;
    unwrap(
      api.librarySearch({
        text: text.trim() === '' ? null : text,
        family: 'inverter',
        includeArchived: false,
        limit: 12,
      }),
    )
      .then((r) => {
        if (live) setRows(r);
      })
      .catch(report);
    return () => {
      live = false;
    };
  }, [text]);
  const add = (c: ComponentSummary_Serialize) =>
    setWiring({
      ...wiring,
      inverters: [
        ...wiring.inverters,
        {
          id: crypto.randomUUID(),
          name: `WR${String(wiring.inverters.length + 1)}`,
          component: c.id,
        },
      ],
    });
  return (
    <Panel title={t('connection.addInverter')}>
      <input
        className="form-input"
        type="search"
        placeholder={t('connection.searchInverter')}
        aria-label={t('connection.searchInverter')}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
        }}
      />
      <ul className="obstacle-list">
        {rows.map((r) => (
          <li key={r.id} className="obstacle-list__item">
            <span>
              {r.label}
              {!r.verified && <span className="panel-hint"> · {t('library.unverified')}</span>}
            </span>
            <Button variant="secondary" onClick={() => void add(r)}>
              {t('connection.add')}
            </Button>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

function SettingsForm({ wiring }: { wiring: Wiring_Serialize }) {
  const { t } = useTranslation();
  const s = wiring.settings;
  const set = (patch: Partial<WiringSettings>) =>
    setWiring({ ...wiring, settings: { ...s, ...patch } });
  const num =
    (apply: (v: number) => Promise<boolean>) =>
    (v: number | null): Promise<boolean> =>
      v === null ? Promise.resolve(false) : apply(v);
  return (
    <Panel title={t('connection.settings')}>
      <div className="form-grid">
        <NumberField
          label={t('connection.minTemp')}
          unit="°C"
          decimals={0}
          value={s.minCellTempC}
          min={-60}
          max={40}
          onCommit={num((v) => set({ minCellTempC: v }))}
        />
        <NumberField
          label={t('connection.maxTemp')}
          unit="°C"
          decimals={0}
          value={s.maxCellTempC}
          min={0}
          max={120}
          onCommit={num((v) => set({ maxCellTempC: v }))}
        />
        <NumberField
          label={t('connection.ratioMin')}
          unit="%"
          decimals={0}
          value={s.dcAcMin * 100}
          min={10}
          max={300}
          onCommit={num((v) => set({ dcAcMin: v / 100 }))}
        />
        <NumberField
          label={t('connection.ratioMax')}
          unit="%"
          decimals={0}
          value={s.dcAcMax * 100}
          min={10}
          max={300}
          onCommit={num((v) => set({ dcAcMax: v / 100 }))}
        />
        <NumberField
          label={t('connection.vocFactor')}
          decimals={2}
          value={s.vocFactor}
          min={1}
          max={1.5}
          onCommit={num((v) => set({ vocFactor: v }))}
        />
      </div>
    </Panel>
  );
}
