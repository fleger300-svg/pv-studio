import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Select } from '@pv/ui';
import type {
  Building,
  ConsequenceClass,
  ServiceLifeModel,
  SiteParams,
  SnowExposure,
  TerrainCategory,
} from '../bindings';
import { runCommand } from '../commands/runCommand';
import { commitBuildingOnly } from '../editor2d/buildingCommands';
import { NumberField } from '../forms/fields';
import { useProjectStore } from '../store/project';

const TERRAINS: readonly TerrainCategory[] = ['II', 'IIa', 'III', 'IV'];
const EXPOSURES: readonly SnowExposure[] = ['windswept', 'normal', 'sheltered'];
const CLASSES: readonly ConsequenceClass[] = ['CC1', 'CC2', 'CC3'];

/**
 * Site data of a new building (as `SiteParams::new`): the location-dependent values
 * (altitude, q_p0) are entered by the user, the others start at the usual defaults.
 */
export function newSite(altitudeM: number, windReferencePressureKnM2: number): SiteParams {
  return {
    altitudeM,
    snowAltitudeCorrectionM: 0,
    windReferencePressureKnM2,
    terrain: 'III',
    snowExposure: 'normal',
    thermalCoefficient: 1,
    serviceLifeYears: 50,
    consequenceClass: 'CC2',
    serviceLifeModel: { kind: 'en1991' },
  };
}

/** Site data of the building (SIA 261 snow and wind). Changing them reruns the statics. */
export function SiteSection({ building: b }: { building: Building }) {
  const { t } = useTranslation();
  const site = b.site ?? null;
  return (
    <>
      <h3 className="panel-subtitle">{t('site.title')}</h3>
      {site ? <SiteForm building={b} site={site} /> : <NewSite building={b} />}
    </>
  );
}

/** No site yet: altitude and q_p0 must be given (no silent default for them). */
function NewSite({ building: b }: { building: Building }) {
  const { t } = useTranslation();
  const [altitude, setAltitude] = useState<number | null>(null);
  const [qp0, setQp0] = useState<number | null>(null);
  const commit = (a: number | null, q: number | null) =>
    a !== null && q !== null
      ? commitBuildingOnly({ ...b, site: newSite(a, q) })
      : Promise.resolve(true);
  return (
    <>
      <p className="panel-hint">{t('site.missing')}</p>
      <NumberField
        label={t('site.altitude')}
        unit="m"
        decimals={0}
        value={altitude}
        allowEmpty
        min={0}
        max={4800}
        onCommit={(v) => {
          setAltitude(v);
          return commit(v, qp0);
        }}
      />
      <NumberField
        label={t('site.qp0')}
        unit="kN/m²"
        value={qp0}
        allowEmpty
        min={0.3}
        max={5}
        onCommit={(v) => {
          setQp0(v);
          return commit(altitude, v);
        }}
      />
    </>
  );
}

function SiteForm({ building: b, site }: { building: Building; site: SiteParams }) {
  const { t } = useTranslation();
  // Select the stable summary, derive outside: a selector returning a new array on every
  // call makes React re-render forever.
  const buildings = useProjectStore((s) => s.summary?.buildings);
  const others = (buildings ?? []).filter((x) => x.id !== b.id);
  const set = (patch: Partial<SiteParams>) =>
    commitBuildingOnly({ ...b, site: { ...site, ...patch } });
  const num =
    (apply: (v: number) => Promise<boolean>) =>
    (v: number | null): Promise<boolean> =>
      v === null ? Promise.resolve(false) : apply(v);
  const explicit = site.serviceLifeModel.kind === 'explicit' ? site.serviceLifeModel : null;
  return (
    <>
      <div className="form-grid">
        <NumberField
          label={t('site.altitude')}
          unit="m"
          decimals={0}
          value={site.altitudeM}
          min={0}
          max={4800}
          onCommit={num((v) => set({ altitudeM: v }))}
        />
        <NumberField
          label={t('site.snowCorrection')}
          unit="m"
          decimals={0}
          value={site.snowAltitudeCorrectionM}
          min={-1000}
          max={2000}
          onCommit={num((v) => set({ snowAltitudeCorrectionM: v }))}
        />
        <NumberField
          label={t('site.qp0')}
          unit="kN/m²"
          value={site.windReferencePressureKnM2}
          min={0.3}
          max={5}
          onCommit={num((v) => set({ windReferencePressureKnM2: v }))}
        />
        <NumberField
          label={t('site.thermal')}
          value={site.thermalCoefficient}
          min={0.5}
          max={1.2}
          onCommit={num((v) => set({ thermalCoefficient: v }))}
        />
        <NumberField
          label={t('site.serviceLife')}
          unit={t('site.years')}
          decimals={0}
          value={site.serviceLifeYears}
          min={1}
          max={200}
          onCommit={num((v) => set({ serviceLifeYears: v }))}
        />
      </div>
      <Select<TerrainCategory>
        label={t('site.terrain')}
        value={site.terrain}
        options={TERRAINS.map((v) => ({ value: v, label: t(`site.terrains.${v}`) }))}
        onChange={(terrain) => void set({ terrain })}
      />
      <Select<SnowExposure>
        label={t('site.exposure')}
        value={site.snowExposure}
        options={EXPOSURES.map((v) => ({ value: v, label: t(`site.exposures.${v}`) }))}
        onChange={(snowExposure) => void set({ snowExposure })}
      />
      <Select<ConsequenceClass>
        label={t('site.consequence')}
        value={site.consequenceClass}
        options={CLASSES.map((v) => ({ value: v, label: v }))}
        onChange={(consequenceClass) => void set({ consequenceClass })}
      />
      <Select<ServiceLifeModel['kind']>
        label={t('site.lifeModel')}
        value={site.serviceLifeModel.kind}
        options={[
          { value: 'en1991', label: t('site.lifeModels.en1991') },
          { value: 'explicit', label: t('site.lifeModels.explicit') },
        ]}
        onChange={(kind) =>
          void set({
            serviceLifeModel: kind === 'explicit' ? { kind, snow: 1, wind: 1 } : { kind: 'en1991' },
          })
        }
      />
      {explicit && (
        <div className="form-grid">
          <NumberField
            label={t('site.snowFactor')}
            value={explicit.snow}
            min={0.5}
            max={1.5}
            onCommit={num((v) => set({ serviceLifeModel: { ...explicit, snow: v } }))}
          />
          <NumberField
            label={t('site.windFactor')}
            value={explicit.wind}
            min={0.5}
            max={1.5}
            onCommit={num((v) => set({ serviceLifeModel: { ...explicit, wind: v } }))}
          />
        </div>
      )}
      {others.length > 0 && (
        <Button
          variant="secondary"
          onClick={() =>
            void runCommand('apply', {
              type: 'Batch',
              commands: others.map((x) => ({
                type: 'UpsertBuilding' as const,
                building: { ...x, site },
                at: null,
              })),
            })
          }
        >
          {t('site.applyAll')}
        </Button>
      )}
    </>
  );
}
