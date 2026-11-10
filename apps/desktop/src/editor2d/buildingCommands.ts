import type {
  Building,
  ProjectSummary,
  RoofFace,
  RoofKind,
  RoofPreset,
  RoofSpec,
} from '../bindings';
import { runCommand, toCommandError } from '../commands/runCommand';
import { api, unwrap } from '../ipc';
import { useSelectionStore } from '../store/selection';
import { useToastStore } from '../store/toast';
import { centroid } from './planGeometry';
import { isConvex, syncEdges } from './polygonEdit';
import type { Draft } from './tools';
import type { Point } from './viewTransform';

/** A new building: eave height, and a gable roof along its main axis (hipped if concave). */
export const NEW_BUILDING = {
  eaveHeightM: 6,
  preset: 'gable' as RoofPreset,
  pitchDeg: 30,
  overhangM: 0.5,
};

const PRESET_OF: Record<RoofKind, RoofPreset> = {
  gable: 'gable',
  hipped: 'hipped',
  monopitch: 'monopitch',
  flat: 'flat',
};

/**
 * Roof specifications given to buildings whose faces were placed one by one (imports,
 * stress presets) the first time their footprint is edited: the preset of their roof kind,
 * at the steepest pitch of their faces, without overhang.
 */
const fallback = new Map<string, RoofSpec>();

/** The roof specification of a building, if known now (see `ensureSpec`). */
export function specOf(summary: ProjectSummary, buildingId: string): RoofSpec | null {
  const b = summary.buildings.find((x) => x.id === buildingId);
  return b?.roof ?? fallback.get(buildingId) ?? null;
}

/** Makes `specOf` answer for a building without specification (one IPC call). */
export async function ensureSpec(summary: ProjectSummary, buildingId: string): Promise<void> {
  const b = summary.buildings.find((x) => x.id === buildingId);
  if (!b || b.roof || fallback.has(buildingId)) return;
  const tilts = summary.roofFaces
    .filter((f) => f.face.buildingId === buildingId)
    .map((f) => f.face.tiltDeg);
  const pitch = tilts.length > 0 ? Math.max(...tilts) : NEW_BUILDING.pitchDeg;
  const preset = pitch < 1 ? 'flat' : PRESET_OF[b.roofKind];
  fallback.set(buildingId, await unwrap(api.roofPreset(b.footprint, preset, pitch, 0)));
}

const relative = (pts: readonly Point[], o: readonly number[]): [number, number][] =>
  pts.map(([e, n]) => [e - (o[0] ?? 0), n - (o[1] ?? 0)]);

/**
 * The building a draft of an existing building describes (footprint relative to its
 * origin, per-edge roof data following inserted / removed vertices), or `null` when its
 * roof specification is not known yet.
 */
export function draftBuilding(summary: ProjectSummary, draft: Draft): Building | null {
  if (draft.buildingId === null || draft.open) return null;
  const b = summary.buildings.find((x) => x.id === draft.buildingId);
  const spec = b ? specOf(summary, b.id) : null;
  if (!b || !spec) return null;
  if (draft.delta) {
    const [dx, dy] = draft.delta;
    return { ...b, origin: [b.origin[0] + dx, b.origin[1] + dy, b.origin[2]], roof: spec };
  }
  const edges = draft.changes.reduce((e, c) => syncEdges(e, c), spec.edges);
  return {
    ...b,
    footprint: relative(draft.footprint, b.origin),
    roof: { ...spec, edges },
  };
}

/** A new building on a footprint (world points): origin at its centroid, default roof. */
export async function newBuilding(footprint: readonly Point[], name: string): Promise<Building> {
  const [ce, cn] = centroid(footprint);
  const origin: [number, number, number] = [
    Math.round(ce * 100) / 100,
    Math.round(cn * 100) / 100,
    0,
  ];
  const rel = relative(footprint, origin);
  const roof = await unwrap(
    // A gable wall cannot stand at a re-entrant corner: concave footprints start hipped.
    api.roofPreset(
      rel,
      isConvex(rel) ? NEW_BUILDING.preset : 'hipped',
      NEW_BUILDING.pitchDeg,
      NEW_BUILDING.overhangM,
    ),
  );
  return {
    id: crypto.randomUUID(),
    name,
    origin,
    footprint: rel,
    sizeM: [1, 1],
    eaveHeightM: NEW_BUILDING.eaveHeightM,
    ridgeHeightM: NEW_BUILDING.eaveHeightM,
    roofKind: 'gable',
    roof,
  };
}

/**
 * Applies a finished gesture: the building with its generated roof, as one command (one
 * undo). The building ends up selected. Failures are reported as toasts; `false` then.
 */
export async function commitDraft(
  summary: ProjectSummary,
  draft: Draft,
  newName: string,
): Promise<boolean> {
  try {
    let building: Building | null;
    if (draft.buildingId === null) {
      building = await newBuilding(draft.footprint, newName);
    } else {
      await ensureSpec(summary, draft.buildingId);
      building = draftBuilding(summary, draft);
    }
    if (!building) return false;
    const ok = await commitBuilding(building);
    if (ok) useSelectionStore.getState().selectBuilding(building.id);
    return ok;
  } catch (e) {
    useToastStore.getState().push(toCommandError(e));
    return false;
  }
}

/**
 * A building (new or changed) with its regenerated roof, as one command. Failures are
 * reported as toasts; `false` then.
 */
export async function commitBuilding(building: Building): Promise<boolean> {
  try {
    const cmd = await unwrap(api.planRoof(building));
    return (await runCommand('apply', cmd)) !== null;
  } catch (e) {
    useToastStore.getState().push(toCommandError(e));
    return false;
  }
}

/** A change of a building that does not touch its roof (its name): one command. */
export async function commitBuildingOnly(building: Building): Promise<boolean> {
  return (await runCommand('apply', { type: 'UpsertBuilding', building, at: null })) !== null;
}

/** A changed roof face (name, covering, obstacles): one command. */
export async function commitFace(face: RoofFace): Promise<boolean> {
  return (await runCommand('apply', { type: 'UpsertRoofFace', face, at: null })) !== null;
}

/** Name of the next new building: « Bâtiment N », N one past the highest in use. */
export function nextBuildingName(summary: ProjectSummary, label: (n: number) => string): string {
  const used = new Set(summary.buildings.map((b) => b.name));
  let n = summary.buildings.length + 1;
  while (used.has(label(n))) n += 1;
  return label(n);
}
