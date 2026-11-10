import { gridStep, type Point } from './viewTransform';

/** A guide line shown while snapping (world points). */
export interface Guide {
  a: Point;
  b: Point;
}

export interface SnapInput {
  point: Point;
  /** Pixels per metre (tolerances are in pixels). */
  scale: number;
  /** Vertices to snap onto, and to align with. */
  vertices: readonly Point[];
  /** The previous point of the stroke: angles are measured from it. */
  anchor?: Point | null;
  /** Extra directions (radians) to snap to from the anchor (existing edges, ±90°). */
  directions?: readonly number[];
  /** Alt held: no snapping at all. */
  disabled?: boolean;
}

export interface SnapResult {
  point: Point;
  guides: Guide[];
  /** What the point snapped to (for the cursor and the tests). */
  kind: 'none' | 'vertex' | 'align' | 'angle' | 'grid';
}

/** Snap distance, pixels. */
export const SNAP_PX = 8;

/** Snapping step of the grid (m): a tenth of the visible grid, at least 1 cm. */
export function snapStep(scale: number): number {
  return Math.max(0.01, gridStep(scale) / 10);
}

const round = (x: number, step: number) => Math.round(x / step) * step;

/**
 * Snaps a point, by priority: an existing vertex; alignment with vertices (east and/or
 * north, with guides); a 45° multiple or an existing edge direction from the anchor; the
 * grid. Alignment on one axis keeps the grid on the other.
 */
export function snap(input: SnapInput): SnapResult {
  const { point, scale, vertices, anchor, directions = [], disabled } = input;
  if (disabled) return { point, guides: [], kind: 'none' };
  const tol = SNAP_PX / scale;
  const step = snapStep(scale);

  // 1. Vertex.
  let best: Point | null = null;
  let bestD = tol;
  for (const v of vertices) {
    const d = Math.hypot(v[0] - point[0], v[1] - point[1]);
    if (d <= bestD) {
      best = v;
      bestD = d;
    }
  }
  if (best) return { point: best, guides: [], kind: 'vertex' };

  // 2. Alignment with vertices (each axis independently).
  let ax: Point | null = null;
  let ay: Point | null = null;
  for (const v of vertices) {
    if (
      Math.abs(v[0] - point[0]) <= tol &&
      (!ax || Math.abs(v[0] - point[0]) < Math.abs(ax[0] - point[0]))
    )
      ax = v;
    if (
      Math.abs(v[1] - point[1]) <= tol &&
      (!ay || Math.abs(v[1] - point[1]) < Math.abs(ay[1] - point[1]))
    )
      ay = v;
  }
  if (ax || ay) {
    const x = ax ? ax[0] : round(point[0], step);
    const y = ay ? ay[1] : round(point[1], step);
    const guides: Guide[] = [];
    if (ax) guides.push({ a: ax, b: [x, y] });
    if (ay) guides.push({ a: ay, b: [x, y] });
    return { point: [x, y], guides, kind: 'align' };
  }

  // 3. Angle from the anchor.
  if (anchor) {
    const dx = point[0] - anchor[0];
    const dy = point[1] - anchor[1];
    const r = Math.hypot(dx, dy);
    if (r > tol) {
      const angles = [...Array.from({ length: 8 }, (_, k) => (k * Math.PI) / 4)];
      for (const d of directions) angles.push(d, d + Math.PI / 2, d + Math.PI, d - Math.PI / 2);
      let bestA: number | null = null;
      let bestOff = tol;
      for (const a of angles) {
        const ux = Math.cos(a);
        const uy = Math.sin(a);
        const along = dx * ux + dy * uy;
        if (along <= 0) continue;
        const off = Math.abs(-dx * uy + dy * ux);
        if (off <= bestOff) {
          bestA = a;
          bestOff = off;
        }
      }
      if (bestA !== null) {
        const ux = Math.cos(bestA);
        const uy = Math.sin(bestA);
        // Length on the grid step along the ray.
        const len = round(dx * ux + dy * uy, step);
        const p: Point = [anchor[0] + ux * len, anchor[1] + uy * len];
        return { point: p, guides: [{ a: anchor, b: p }], kind: 'angle' };
      }
    }
  }

  // 4. Grid.
  return { point: [round(point[0], step), round(point[1], step)], guides: [], kind: 'grid' };
}
