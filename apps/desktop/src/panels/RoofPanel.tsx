import { useTranslation } from 'react-i18next';
import { Button, Panel, Select, Stat } from '@pv/ui';
import type { ObstacleKind, RoofFace, RoofObstacle, SupportGridKind } from '../bindings';
import { commitFace } from '../editor2d/buildingCommands';
import {
  boxOf,
  footprintOf,
  newObstacle,
  outlineOf,
  withObstacles,
  type ObstacleBox,
} from '../editor2d/obstacles';
import { signedArea } from '../editor2d/polygonEdit';
import { NumberField, TextField } from '../forms/fields';
import { useProjectStore } from '../store/project';
import { useSelectionStore } from '../store/selection';

const OBSTACLE_KINDS: readonly ObstacleKind[] = [
  'chimney',
  'skylight',
  'vent',
  'dormer',
  'keepOut',
];
type Covering = 'none' | SupportGridKind;
const COVERINGS: readonly Covering[] = ['none', 'standingSeam', 'rafters'];
/** Typical spacing of seams / rafters when a covering is chosen, m. */
const DEFAULT_SPACING: Record<SupportGridKind, number> = { standingSeam: 0.5, rafters: 0.8 };

/** Roof faces of the selected building; the selected face's name, covering, obstacles. */
export function RoofPanel() {
  const { t, i18n } = useTranslation();
  const summary = useProjectStore((s) => s.summary);
  const buildingId = useSelectionStore((s) => s.buildingId);
  const faceId = useSelectionStore((s) => s.faceId);
  const selectFace = useSelectionStore((s) => s.selectFace);
  const fmt = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });
  const faces = summary?.roofFaces.filter((f) => f.face.buildingId === buildingId) ?? [];
  if (!summary || !buildingId) {
    return (
      <Panel title={t('roof.facesTitle')}>
        <p className="panel-hint">{t('building.none')}</p>
      </Panel>
    );
  }
  const face = faces.find((f) => f.face.id === faceId)?.face ?? null;
  return (
    <>
      <Panel title={t('roof.facesTitle')}>
        <ul className="face-list">
          {faces.map(({ face: f }) => (
            <li key={f.id}>
              <button
                type="button"
                className="face-list__item"
                aria-current={f.id === faceId ? 'true' : undefined}
                onClick={() => {
                  selectFace(f.id, f.buildingId);
                }}
              >
                <span>{f.name}</span>
                <span className="face-list__meta">
                  {t('roof.faceMeta', {
                    tilt: fmt.format(f.tiltDeg),
                    azimuth: fmt.format(f.azimuthDeg),
                  })}
                </span>
              </button>
            </li>
          ))}
        </ul>
        {faces.length === 0 && <p className="panel-hint">{t('roof.noFaces')}</p>}
      </Panel>
      {face ? (
        <FaceForm key={face.id} face={face} />
      ) : (
        faces.length > 0 && (
          <Panel title={t('roof.faceTitle')}>
            <p className="panel-hint">{t('roof.noFace')}</p>
          </Panel>
        )
      )}
    </>
  );
}

function FaceForm({ face }: { face: RoofFace }) {
  const { t, i18n } = useTranslation();
  const fmt = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });
  const outline = outlineOf(face);
  const area = Math.abs(signedArea(outline.verticesUv.map(([u, v]) => [u, v] as const)));
  const grid = face.supportGrid ?? null;
  const covering: Covering = grid?.kind ?? 'none';
  const obstacles = outline.obstacles;

  const setObstacles = (next: RoofObstacle[]) => commitFace(withObstacles(face, next));
  const setObstacle = (i: number, o: RoofObstacle) =>
    setObstacles(obstacles.map((x, k) => (k === i ? o : x)));
  const setBox = (i: number, o: RoofObstacle, patch: Partial<ObstacleBox>) =>
    setObstacle(i, { ...o, footprintUv: footprintOf({ ...boxOf(o.footprintUv), ...patch }) });

  return (
    <Panel title={t('roof.faceTitle')}>
      <TextField
        label={t('roof.faceName')}
        value={face.name}
        onCommit={(name) => commitFace({ ...face, name })}
      />
      <Stat label={t('roof.tilt')} value={`${fmt.format(face.tiltDeg)}°`} />
      <Stat label={t('roof.azimuth')} value={`${fmt.format(face.azimuthDeg)}°`} />
      <Stat label={t('roof.area')} value={`${fmt.format(area)} m²`} />
      <Select<Covering>
        label={t('roof.covering')}
        value={covering}
        options={COVERINGS.map((c) => ({ value: c, label: t(`roof.coverings.${c}`) }))}
        onChange={(c) => {
          void commitFace({
            ...face,
            supportGrid:
              c === 'none'
                ? null
                : {
                    kind: c,
                    originM: grid?.originM ?? 0,
                    spacingM: grid?.spacingM ?? DEFAULT_SPACING[c],
                  },
          });
        }}
      />
      {grid && (
        <>
          <NumberField
            label={t('roof.spacing')}
            unit="m"
            value={grid.spacingM}
            min={0.1}
            max={5}
            decimals={3}
            onCommit={(v) =>
              v === null
                ? Promise.resolve(false)
                : commitFace({ ...face, supportGrid: { ...grid, spacingM: v } })
            }
          />
          <NumberField
            label={t('roof.gridOrigin')}
            unit="m"
            value={grid.originM}
            min={-50}
            max={50}
            decimals={3}
            onCommit={(v) =>
              v === null
                ? Promise.resolve(false)
                : commitFace({ ...face, supportGrid: { ...grid, originM: v } })
            }
          />
        </>
      )}
      <h3 className="panel-subtitle">{t('obstacles.title')}</h3>
      {obstacles.length === 0 && <p className="panel-hint">{t('obstacles.none')}</p>}
      <ol className="obstacle-list">
        {obstacles.map((o, i) => {
          const box = boxOf(o.footprintUv);
          return (
            <li key={i} className="obstacle-list__item">
              <TextField
                label={t('obstacles.name')}
                value={o.name}
                onCommit={(name) => setObstacle(i, { ...o, name })}
              />
              <Select<ObstacleKind>
                label={t('obstacles.kind')}
                value={o.kind}
                options={OBSTACLE_KINDS.map((k) => ({
                  value: k,
                  label: t(`obstacles.kinds.${k}`),
                }))}
                onChange={(kind) => {
                  void setObstacle(i, { ...o, kind });
                }}
              />
              <div className="form-grid">
                <NumberField
                  label={t('obstacles.u')}
                  unit="m"
                  value={box.u}
                  min={-5}
                  max={face.widthM + 5}
                  onCommit={(v) => (v === null ? Promise.resolve(false) : setBox(i, o, { u: v }))}
                />
                <NumberField
                  label={t('obstacles.v')}
                  unit="m"
                  value={box.v}
                  min={-5}
                  max={face.heightM + 5}
                  onCommit={(v) => (v === null ? Promise.resolve(false) : setBox(i, o, { v }))}
                />
                <NumberField
                  label={t('obstacles.width')}
                  unit="m"
                  value={box.width}
                  min={0.05}
                  max={50}
                  onCommit={(v) =>
                    v === null ? Promise.resolve(false) : setBox(i, o, { width: v })
                  }
                />
                <NumberField
                  label={t('obstacles.length')}
                  unit="m"
                  value={box.length}
                  min={0.05}
                  max={50}
                  onCommit={(v) =>
                    v === null ? Promise.resolve(false) : setBox(i, o, { length: v })
                  }
                />
                <NumberField
                  label={t('obstacles.height')}
                  unit="m"
                  value={o.heightM}
                  min={0}
                  max={20}
                  onCommit={(v) =>
                    v === null ? Promise.resolve(false) : setObstacle(i, { ...o, heightM: v })
                  }
                />
              </div>
              <Button
                variant="secondary"
                onClick={() => void setObstacles(obstacles.filter((_, k) => k !== i))}
              >
                {t('obstacles.delete')}
              </Button>
            </li>
          );
        })}
      </ol>
      <Button
        variant="secondary"
        onClick={() =>
          void setObstacles([
            ...obstacles,
            newObstacle(face, 'chimney', t('obstacles.defaultName', { n: obstacles.length + 1 })),
          ])
        }
      >
        {t('obstacles.add')}
      </Button>
    </Panel>
  );
}
