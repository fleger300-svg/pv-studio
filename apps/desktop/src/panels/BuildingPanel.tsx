import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button, Panel, Select, Stat } from '@pv/ui';
import type {
  Building,
  EdgeRoof,
  ProjectSummary,
  RoofForm,
  RoofShape,
  RoofSpec,
} from '../bindings';
import { runCommand } from '../commands/runCommand';
import {
  commitBuilding,
  commitBuildingOnly,
  ensureSpec,
  specOf,
} from '../editor2d/buildingCommands';
import { edgeLength } from '../editor2d/polygonEdit';
import { CheckField, NumberField, TextField } from '../forms/fields';
import { api, unwrap } from '../ipc';
import { usePlanStore } from '../store/plan';
import { SiteSection } from './SiteSection';
import { useProjectStore } from '../store/project';
import { useSelectionStore } from '../store/selection';

const SHAPES: readonly RoofShape[] = ['gable', 'hipped', 'halfHipped', 'monopitch', 'flat'];

/** The roof specification of a building and its form (type and numbers), kept up to date. */
function useRoof(summary: ProjectSummary, b: Building) {
  const [spec, setSpec] = useState<RoofSpec | null>(() => specOf(summary, b.id));
  const [form, setForm] = useState<RoofForm | null>(null);
  useEffect(() => {
    const cancelled = new AbortController();
    void (async () => {
      await ensureSpec(summary, b.id);
      const s = specOf(summary, b.id);
      if (!s) return;
      const f = await unwrap(api.roofForm(b.footprint, s, b.eaveHeightM));
      if (cancelled.signal.aborted) return;
      setSpec(s);
      setForm(f);
    })();
    return () => {
      cancelled.abort();
    };
  }, [summary, b]);
  return { spec, form };
}

/** The selected building: name, heights, roof type and pitches, overhang, edges. */
export function BuildingPanel() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const selectedId = useSelectionStore((s) => s.buildingId);
  const building = summary?.buildings.find((b) => b.id === selectedId) ?? null;
  if (!summary || !building) {
    return (
      <Panel title={t('building.title')}>
        <p className="panel-hint">{t('building.none')}</p>
      </Panel>
    );
  }
  // Keyed: switching building resets the fields.
  return <BuildingForm key={building.id} summary={summary} building={building} />;
}

function BuildingForm({ summary, building: b }: { summary: ProjectSummary; building: Building }) {
  const { t, i18n } = useTranslation();
  const { spec, form } = useRoof(summary, b);
  const [advanced, setAdvanced] = useState(false);
  const fmt = useMemo(
    () => new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 }),
    [i18n.language],
  );

  /** The building with a new form (roof regenerated). */
  const withForm = async (next: RoofForm, extra: Partial<Building> = {}) => {
    const s = await unwrap(api.roofFromForm(b.footprint, next));
    if (!s) return false;
    return commitBuilding({ ...b, ...extra, roof: s });
  };
  const withEdges = (edges: EdgeRoof[]) =>
    spec ? commitBuilding({ ...b, roof: { ...spec, edges } }) : Promise.resolve(false);

  const custom = form?.shape === 'custom';
  const shapeOptions = [
    ...SHAPES.map((s) => ({ value: s, label: t(`roof.shapes.${s}`) })),
    ...(custom ? [{ value: 'custom' as RoofShape, label: t('roof.shapes.custom') }] : []),
  ];
  const sloped = form && form.shape !== 'flat';
  const ends = form && (form.shape === 'hipped' || form.shape === 'halfHipped');

  return (
    <Panel title={t('building.title')}>
      <TextField
        label={t('building.name')}
        value={b.name}
        onCommit={(name) => commitBuildingOnly({ ...b, name })}
      />
      <NumberField
        label={t('building.eaveHeight')}
        unit="m"
        value={b.eaveHeightM}
        min={1}
        max={60}
        onCommit={(v) =>
          v === null
            ? Promise.resolve(false)
            : spec
              ? commitBuilding({ ...b, eaveHeightM: v, roof: spec })
              : Promise.resolve(false)
        }
      />
      {form && (
        <>
          <Select<RoofShape>
            label={t('roof.shape')}
            value={form.shape}
            options={shapeOptions}
            onChange={(shape) => {
              if (shape === 'custom') return;
              void withForm({
                ...form,
                shape,
                // A half-hip starts 1.5 m above the eave.
                endEaveM:
                  shape === 'halfHipped' && form.endEaveM <= b.eaveHeightM
                    ? b.eaveHeightM + 1.5
                    : form.endEaveM,
              });
            }}
          />
          {sloped && (
            <NumberField
              label={custom ? t('roof.pitchAll') : t('roof.pitch')}
              unit="°"
              decimals={1}
              value={form.pitchDeg}
              min={1}
              max={75}
              onCommit={(v) => {
                if (v === null) return Promise.resolve(false);
                if (custom && spec) {
                  // Custom: every sloped edge takes the new pitch, gables stay.
                  return withEdges(
                    spec.edges.map((e) => (e.pitchDeg < 90 ? { ...e, pitchDeg: v } : e)),
                  );
                }
                return withForm({ ...form, pitchDeg: v });
              }}
            />
          )}
          {ends && (
            <NumberField
              label={t('roof.endPitch')}
              unit="°"
              decimals={1}
              value={form.endPitchDeg}
              min={1}
              max={85}
              onCommit={(v) =>
                v === null ? Promise.resolve(false) : withForm({ ...form, endPitchDeg: v })
              }
            />
          )}
          {form.shape === 'halfHipped' && (
            <NumberField
              label={t('roof.endEave')}
              unit="m"
              value={form.endEaveM}
              min={b.eaveHeightM}
              max={b.eaveHeightM + 30}
              onCommit={(v) =>
                v === null ? Promise.resolve(false) : withForm({ ...form, endEaveM: v })
              }
            />
          )}
          {form.shape === 'monopitch' && (
            <Select<string>
              label={t('roof.lowEdge')}
              value={String(form.lowEdge)}
              options={b.footprint.map((_, i) => ({
                value: String(i),
                label: t('roof.edgeLabel', {
                  n: i + 1,
                  length: fmt.format(edgeLength(b.footprint, i)),
                }),
              }))}
              onChange={(v) => {
                void withForm({ ...form, lowEdge: Number(v) });
              }}
            />
          )}
          <NumberField
            label={t('roof.overhang')}
            unit="m"
            value={form.overhangM}
            min={0}
            max={3}
            onCommit={(v) =>
              v === null || !spec
                ? Promise.resolve(false)
                : custom
                  ? commitBuilding({ ...b, roof: { ...spec, overhangM: v } })
                  : withForm({ ...form, overhangM: v })
            }
          />
        </>
      )}
      <Stat label={t('roof.kind')} value={t(`roof.kinds.${b.roofKind}`)} />
      <Stat label={t('building.ridgeHeight')} value={`${fmt.format(b.ridgeHeightM)} m`} />
      <Stat
        label={t('building.size')}
        value={`${fmt.format(b.sizeM[0])} × ${fmt.format(b.sizeM[1])} m`}
      />
      <Button
        variant="secondary"
        aria-expanded={advanced}
        onClick={() => {
          setAdvanced(!advanced);
        }}
      >
        {advanced ? t('roof.edges.hide') : t('roof.edges.show')}
      </Button>
      {advanced && spec && <EdgeTable building={b} spec={spec} onEdges={withEdges} />}
      <SiteSection building={b} />
      <Button
        variant="secondary"
        onClick={() => void runCommand('apply', { type: 'RemoveBuilding', id: b.id })}
      >
        {t('building.delete')}
      </Button>
    </Panel>
  );
}

/** Per-edge roof data: gable wall, pitch, eave height (empty: the building's). */
function EdgeTable({
  building: b,
  spec,
  onEdges,
}: {
  building: Building;
  spec: RoofSpec;
  onEdges: (edges: EdgeRoof[]) => Promise<boolean>;
}) {
  const { t, i18n } = useTranslation();
  const setHighlight = usePlanStore((s) => s.setHighlightEdge);
  const fmt = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 });
  const set = (i: number, e: EdgeRoof) => onEdges(spec.edges.map((x, k) => (k === i ? e : x)));
  return (
    <ol className="edge-table" aria-label={t('roof.edges.title')}>
      {spec.edges.map((e, i) => (
        <li
          key={i}
          className="edge-table__row"
          onMouseEnter={() => {
            setHighlight(i);
          }}
          onMouseLeave={() => {
            setHighlight(null);
          }}
        >
          <span className="edge-table__title">
            {t('roof.edgeLabel', { n: i + 1, length: fmt.format(edgeLength(b.footprint, i)) })}
          </span>
          <CheckField
            label={t('roof.edges.gable')}
            checked={e.pitchDeg >= 90}
            onCommit={(gable) => set(i, { ...e, pitchDeg: gable ? 90 : 30 })}
          />
          {e.pitchDeg < 90 && (
            <NumberField
              label={t('roof.pitch')}
              unit="°"
              decimals={1}
              value={e.pitchDeg}
              min={1}
              max={85}
              onFocusChange={(f) => {
                setHighlight(f ? i : null);
              }}
              onCommit={(v) =>
                v === null ? Promise.resolve(false) : set(i, { ...e, pitchDeg: v })
              }
            />
          )}
          <NumberField
            label={t('roof.edges.eave')}
            unit="m"
            value={e.eaveHeightM ?? null}
            placeholder={fmt.format(b.eaveHeightM)}
            allowEmpty
            min={0}
            max={60}
            onFocusChange={(f) => {
              setHighlight(f ? i : null);
            }}
            onCommit={(v) => set(i, { ...e, eaveHeightM: v })}
          />
        </li>
      ))}
    </ol>
  );
}
