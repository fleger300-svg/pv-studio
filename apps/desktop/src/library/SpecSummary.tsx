import { useTranslation } from 'react-i18next';
import { Stat } from '@pv/ui';
import type { Component_Serialize } from '../bindings';

type Spec = Component_Serialize['spec'];

/** Read-only key figures of a component's datasheet, by family, and its points to check. */
export function SpecSummary({ spec }: { spec: Spec }) {
  const { t, i18n } = useTranslation();
  const fmt = (v: number | null | undefined, unit = '', digits = 2) =>
    v === null || v === undefined
      ? '—'
      : `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: digits }).format(v)}${unit ? ` ${unit}` : ''}`;
  const rows: [string, string][] = [];
  let notes: string[];
  let ref: string | null | undefined;

  switch (spec.family) {
    case 'module': {
      const d = spec.details;
      rows.push(
        [t('library.spec.series'), d.series ?? '—'],
        [t('library.spec.kind'), t(`library.spec.moduleKinds.${d.kind}`)],
        [t('library.spec.efficiency'), fmt(d.efficiencyPct, '%')],
        [t('library.spec.noct'), fmt(d.nominalOperatingTempC, '°C', 0)],
        [
          t('library.spec.warranty'),
          `${fmt(d.warrantyProductYears, '', 0)} / ${fmt(d.warrantyPowerYears, '', 0)}`,
        ],
        [t('library.spec.hail'), d.hailClass ?? '—'],
      );
      notes = d.notes;
      ref = d.datasheetRef;
      break;
    }
    case 'inverter': {
      const tr = spec.dc.trackers;
      rows.push(
        [t('library.spec.inverterKind'), t(`library.spec.inverterKinds.${spec.kind}`)],
        [
          t('library.spec.acPower'),
          `${fmt(spec.ac.nominalPowerW / 1000, 'kW')} · ${String(spec.ac.phases)}~`,
        ],
        [t('library.spec.dcMax'), fmt(spec.dc.maxVoltageV, 'V', 0)],
        [
          t('library.spec.mppt'),
          `${fmt(spec.dc.mpptMinV, '', 0)}–${fmt(spec.dc.mpptMaxV, 'V', 0)}`,
        ],
        [
          t('library.spec.trackers'),
          tr.map((x) => `${String(x.inputs)}× ${fmt(x.maxCurrentA, 'A', 1)}`).join(' · '),
        ],
        [
          t('library.spec.efficiency'),
          `${fmt(spec.maxEfficiencyPct, '%')} (EU ${fmt(spec.euroEfficiencyPct, '%')})`,
        ],
        [t('library.spec.battery'), spec.battery ? t('library.spec.yes') : t('library.spec.no')],
      );
      notes = spec.details.notes;
      ref = spec.details.datasheetRef;
      break;
    }
    case 'optimizer':
      rows.push(
        [t('library.spec.ratedInput'), fmt(spec.ratedInputPowerW, 'W', 0)],
        [t('library.spec.maxInputVoltage'), fmt(spec.maxInputVoltageV, 'V', 0)],
        [t('library.spec.maxInputIsc'), fmt(spec.maxInputShortCircuitCurrentA, 'A', 1)],
        [t('library.spec.stringRules'), String(spec.stringRules.length)],
      );
      notes = spec.details.notes;
      ref = spec.details.datasheetRef;
      break;
    case 'battery':
      rows.push(
        [t('library.spec.chemistry'), `${spec.chemistry} · ${spec.coupling.toUpperCase()}`],
        [
          t('library.spec.moduleCapacity'),
          fmt(spec.moduleUsableKwh ?? spec.moduleNominalKwh, 'kWh'),
        ],
        [
          t('library.spec.modules'),
          `${String(spec.minModules)}–${String(spec.maxModulesPerStack)}`,
        ],
        [t('library.spec.power'), t(`library.spec.powerRules.${spec.power.kind}`)],
        [t('library.spec.location'), spec.location ?? '—'],
      );
      notes = spec.details.notes;
      ref = spec.details.datasheetRef;
      break;
    case 'equipment':
      rows.push([t('library.spec.equipmentKind'), t(`library.spec.equipmentKinds.${spec.kind}`)]);
      for (const p of spec.properties.slice(0, 12)) {
        rows.push([
          p.name.replace(/_/g, ' '),
          p.value.kind === 'number' ? fmt(p.value.value, p.value.unit) : p.value.value,
        ]);
      }
      notes = spec.details.notes;
      ref = spec.details.datasheetRef;
      break;
    default:
      return null;
  }

  return (
    <div className="spec-summary">
      <div className="form-grid">
        {rows.map(([label, value]) => (
          <Stat key={label} label={label} value={value} />
        ))}
      </div>
      {ref && <p className="panel-hint">{t('library.spec.datasheet', { ref })}</p>}
      {notes.length > 0 && (
        <details className="spec-summary__notes">
          <summary>{t('library.spec.notes', { count: notes.length })}</summary>
          <ul>
            {notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
