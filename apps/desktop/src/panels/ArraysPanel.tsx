import { useTranslation } from 'react-i18next';
import { Button, Panel, Select, Stat } from '@pv/ui';
import type { Orientation, PanelArray, RoofFaceId } from '../bindings';
import { runApplied, runCommand, toCommandError } from '../commands/runCommand';
import { NumberField } from '../forms/fields';
import { api, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useSelectionStore } from '../store/selection';
import { useToastStore } from '../store/toast';

/** Lays out the array's face again with the array as given: one command (one undo). */
async function layOut(array: PanelArray): Promise<boolean> {
  // Planned and applied in Rust (the batch is journalled there): one call, one undo.
  return (await runApplied('apply', () => unwrap(api.applyLayout(array)))) !== null;
}

async function addArray(faceId: RoofFaceId): Promise<boolean> {
  try {
    return await layOut(await unwrap(api.defaultArray(faceId)));
  } catch (e) {
    useToastStore.getState().push(toCommandError(e));
    return false;
  }
}

/**
 * Arrays of the selected roof face (click a roof in the 3D view). Changing a setting lays
 * the face out again (every array of the face, as one undoable batch).
 */
export function ArraysPanel() {
  const { t } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const faceId = useSelectionStore((s) => s.faceId);
  const face = summary?.roofFaces.find((f) => f.face.id === faceId)?.face ?? null;
  if (!summary || !face) {
    return (
      <Panel title={t('arrays.title')}>
        <p className="panel-hint">{t('arrays.noFace')}</p>
      </Panel>
    );
  }
  const arrays = summary.arrays.filter((a) => a.array.roofFaceId === face.id);
  return (
    <Panel title={t('arrays.titleOn', { face: face.name })}>
      {arrays.length === 0 && <p className="panel-hint">{t('arrays.none')}</p>}
      <ol className="obstacle-list">
        {arrays.map((a, i) => (
          <ArrayForm
            key={a.array.id}
            index={i + 1}
            array={a.array}
            modules={a.moduleCount}
            peakWp={summary.modulePeakPowerWp}
            mountingLabel={a.mountingLabel ?? null}
          />
        ))}
      </ol>
      <Button variant="secondary" onClick={() => void addArray(face.id)}>
        {t('arrays.add')}
      </Button>
    </Panel>
  );
}

function ArrayForm({
  index,
  array: a,
  modules,
  peakWp,
  mountingLabel,
}: {
  index: number;
  array: PanelArray;
  modules: number;
  peakWp: number;
  mountingLabel: string | null;
}) {
  const { t, i18n } = useTranslation();
  const fmt = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 });
  const set = (patch: Partial<PanelArray>) => layOut({ ...a, ...patch });
  const setLayout = (patch: Partial<PanelArray['layout']>) =>
    layOut({ ...a, layout: { ...a.layout, ...patch } });
  const num =
    (apply: (v: number) => Promise<boolean>) =>
    (v: number | null): Promise<boolean> =>
      v === null ? Promise.resolve(false) : apply(v);
  return (
    <li className="obstacle-list__item">
      <span className="edge-table__title">{t('arrays.item', { n: index })}</span>
      <Stat label={t('arrays.modules')} value={String(modules)} />
      <Stat label={t('arrays.power')} value={`${fmt.format((modules * peakWp) / 1000)} kWc`} />
      <Select<Orientation>
        label={t('arrays.orientation')}
        value={a.orientation}
        options={(['portrait', 'landscape'] as const).map((o) => ({
          value: o,
          label: t(`arrays.orientations.${o}`),
        }))}
        onChange={(orientation) => {
          void set({ orientation });
        }}
      />
      <div className="form-grid">
        <NumberField
          label={t('arrays.moduleLength')}
          unit="m"
          decimals={3}
          value={a.moduleLengthM}
          min={0.5}
          max={3}
          onCommit={num((v) => set({ moduleLengthM: v }))}
        />
        <NumberField
          label={t('arrays.moduleWidth')}
          unit="m"
          decimals={3}
          value={a.moduleWidthM}
          min={0.3}
          max={2}
          onCommit={num((v) => set({ moduleWidthM: v }))}
        />
        <NumberField
          label={t('arrays.gapU')}
          unit="m"
          decimals={3}
          value={a.layout.moduleGapUM}
          min={0}
          max={1}
          onCommit={num((v) => setLayout({ moduleGapUM: v }))}
        />
        <NumberField
          label={t('arrays.gapV')}
          unit="m"
          decimals={3}
          value={a.layout.moduleGapVM}
          min={0}
          max={5}
          onCommit={num((v) => setLayout({ moduleGapVM: v }))}
        />
        <NumberField
          label={t('arrays.edgeSetback')}
          unit="m"
          value={a.layout.edgeSetbackM}
          min={0}
          max={5}
          onCommit={num((v) => setLayout({ edgeSetbackM: v }))}
        />
        <NumberField
          label={t('arrays.obstacleClearance')}
          unit="m"
          value={a.layout.obstacleClearanceM}
          min={0}
          max={5}
          onCommit={num((v) => setLayout({ obstacleClearanceM: v }))}
        />
        <NumberField
          label={t('arrays.otherArrays')}
          unit="m"
          value={a.layout.minDistanceToOtherArraysM}
          min={0}
          max={10}
          onCommit={num((v) => setLayout({ minDistanceToOtherArraysM: v }))}
        />
      </div>
      <Stat label={t('arrays.mounting')} value={mountingLabel ?? t('arrays.noMounting')} />
      <Button variant="secondary" onClick={() => void layOut(a)}>
        {t('arrays.relayout')}
      </Button>
      <Button
        variant="secondary"
        disabled={modules === 0}
        onClick={() =>
          void runCommand('apply', { type: 'SetArrayModules', array_id: a.id, modules: [] })
        }
      >
        {t('arrays.clear')}
      </Button>
      <Button
        variant="secondary"
        onClick={() => void runCommand('apply', { type: 'RemoveArray', id: a.id })}
      >
        {t('arrays.delete')}
      </Button>
    </li>
  );
}
