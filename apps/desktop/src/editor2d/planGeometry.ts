import type { CommandError, ProjectSummary, RoofEdgeKind, RoofFrame } from '../bindings';
import { faceOutline } from '../viewport3d/buildingGeometry';
import type { Bounds, Point } from './viewTransform';

/** A building on the plan: its footprint in world ENU (m). */
export interface PlanBuilding {
  id: string;
  name: string;
  footprint: Point[];
}

/** A roof edge on the plan, with its kind (eave, ridge, hip…). */
export interface PlanEdge {
  a: Point;
  b: Point;
  kind: RoofEdgeKind;
}

/** A roof face on the plan: its outline projected on the ground, and its edges. */
export interface PlanFace {
  id: string;
  buildingId: string;
  name: string;
  polygon: Point[];
  edges: PlanEdge[];
  /** Obstacles projected on the ground. */
  obstacles: Point[][];
  /** Downslope compass direction (°) and pitch (°), for the slope arrow. */
  azimuthDeg: number;
  tiltDeg: number;
}

/** Default edge kinds of a face without outline (see `RoofOutline::from_face`). */
export const RECT_PITCHED: RoofEdgeKind[] = ['eave', 'verge', 'ridge', 'verge'];
export const RECT_FLAT: RoofEdgeKind[] = ['parapet', 'parapet', 'parapet', 'parapet'];

/** Roof `(u, v)` → world plan `(east, north)`. */
export function uvToPlan(frame: RoofFrame, u: number, v: number): Point {
  const [ox, oy] = frame.origin;
  return [
    ox + u * frame.axisU[0] + v * frame.axisV[0],
    oy + u * frame.axisU[1] + v * frame.axisV[1],
  ];
}

export function planBuildings(summary: ProjectSummary): PlanBuilding[] {
  return summary.buildings.map((b) => ({
    id: b.id,
    name: b.name,
    footprint: b.footprint.map(([e, n]): Point => [b.origin[0] + e, b.origin[1] + n]),
  }));
}

export function planFaces(summary: ProjectSummary): PlanFace[] {
  return summary.roofFaces.map(({ face, frame }) => {
    const polygon = faceOutline(face).map(([u, v]) => uvToPlan(frame, u, v));
    const kinds = face.outline?.edgeKinds ?? (face.tiltDeg < 5 ? RECT_FLAT : RECT_PITCHED);
    const edges = polygon.map((a, i) => ({
      a,
      b: polygon[(i + 1) % polygon.length] ?? a,
      kind: kinds[i] ?? 'other',
    }));
    return {
      id: face.id,
      buildingId: face.buildingId,
      name: face.name,
      polygon,
      edges,
      obstacles: (face.outline?.obstacles ?? []).map((o) =>
        o.footprintUv.map(([u, v]) => uvToPlan(frame, u, v)),
      ),
      azimuthDeg: face.azimuthDeg,
      tiltDeg: face.tiltDeg,
    };
  });
}

/** Everything drawn on the plan (footprints and roofs), or `null` for an empty site. */
export function planBounds(buildings: PlanBuilding[], faces: PlanFace[]): Bounds | null {
  const points = [...buildings.flatMap((b) => b.footprint), ...faces.flatMap((f) => f.polygon)];
  if (points.length === 0) return null;
  return points.reduce<Bounds>(
    (bb, [e, n]) => ({
      minE: Math.min(bb.minE, e),
      minN: Math.min(bb.minN, n),
      maxE: Math.max(bb.maxE, e),
      maxN: Math.max(bb.maxN, n),
    }),
    { minE: Infinity, minN: Infinity, maxE: -Infinity, maxN: -Infinity },
  );
}

/** Area centroid of a simple polygon (vertex mean when degenerate). */
export function centroid(polygon: readonly Point[]): Point {
  let a = 0;
  let cx = 0;
  let cy = 0;
  polygon.forEach(([x0, y0], i) => {
    const [x1, y1] = polygon[(i + 1) % polygon.length] ?? [x0, y0];
    const c = x0 * y1 - x1 * y0;
    a += c;
    cx += (x0 + x1) * c;
    cy += (y0 + y1) * c;
  });
  if (Math.abs(a) < 1e-12) {
    const n = Math.max(polygon.length, 1);
    return [polygon.reduce((s, p) => s + p[0], 0) / n, polygon.reduce((s, p) => s + p[1], 0) / n];
  }
  return [cx / (3 * a), cy / (3 * a)];
}

/** Whether `p` is inside a polygon (even-odd rule). */
export function contains(polygon: readonly Point[], [x, y]: Point): boolean {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const [xi, yi] = polygon[i] ?? [0, 0];
    const [xj, yj] = polygon[j] ?? [0, 0];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Footprint edges to show in red for a refused preview (all of them if not specific). */
export function errorEdges(error: CommandError | null, n: number): Set<number> {
  if (!error) return new Set();
  if (error.key === 'errors.roof.reentrantCorner') {
    const a = Number(error.params.a) - 1;
    const b = Number(error.params.b) - 1;
    return new Set([a, b].filter((i) => i >= 0 && i < n));
  }
  return new Set(Array.from({ length: n }, (_, i) => i));
}
