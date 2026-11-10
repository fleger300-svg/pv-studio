import type { Point } from './viewTransform';

/**
 * Footprint editing on world points (east, north; m). Every operation that changes the
 * vertex count says how the per-edge data (pitch, gable, eave of each edge) follows:
 * `syncEdges` applies it to any array indexed by edge.
 */
export type Poly = Point[];

/** How per-edge data follows an edit. */
export type EdgeChange =
  | { kind: 'none' }
  | { kind: 'insert'; edge: number } // edge split in two: both halves keep its data
  | { kind: 'remove'; vertex: number } // edges vertex−1 and vertex merged: the first wins
  | { kind: 'reverse' }; // orientation flipped

export function signedArea(p: readonly Point[]): number {
  let a = 0;
  p.forEach(([x0, y0], i) => {
    const [x1, y1] = p[(i + 1) % p.length] ?? [x0, y0];
    a += x0 * y1 - x1 * y0;
  });
  return a / 2;
}

/** Counter-clockwise version of a polygon (the model's orientation). */
export function ensureCcw(p: readonly Point[]): { poly: Poly; change: EdgeChange } {
  if (signedArea(p) >= 0) return { poly: [...p], change: { kind: 'none' } };
  return { poly: [...p].reverse(), change: { kind: 'reverse' } };
}

/** Applies an edge change to per-edge data (`n` edges before the change). */
export function syncEdges<T>(edges: readonly T[], change: EdgeChange): T[] {
  const n = edges.length;
  switch (change.kind) {
    case 'none':
      return [...edges];
    case 'insert': {
      const out = [...edges];
      const e = edges[change.edge];
      if (e !== undefined) out.splice(change.edge + 1, 0, e);
      return out;
    }
    case 'remove': {
      // Edge `vertex` (leaving the removed vertex) disappears; edge `vertex − 1` now runs to
      // the next vertex.
      const out = [...edges];
      out.splice(change.vertex % n, 1);
      return out;
    }
    case 'reverse':
      // Reversed polygon: new edge k joins old vertices n−1−k → n−2−k, the old edge n−2−k.
      return edges.map((_, k) => edges[(2 * n - 2 - k) % n] as T);
  }
}

export function moveVertex(p: readonly Point[], i: number, to: Point): Poly {
  return p.map((q, k) => (k === i ? to : q));
}

/** Inserts `at` after vertex `edge` (on that edge); the new vertex is `edge + 1`. */
export function insertVertex(p: readonly Point[], edge: number, at: Point): Poly {
  const out = [...p];
  out.splice(edge + 1, 0, at);
  return out;
}

/** Removes a vertex (a polygon keeps at least 3); `null` when it cannot. */
export function deleteVertex(p: readonly Point[], i: number): Poly | null {
  if (p.length <= 3) return null;
  return p.filter((_, k) => k !== i);
}

export function translate(p: readonly Point[], dx: number, dy: number): Poly {
  return p.map(([x, y]) => [x + dx, y + dy]);
}

/** Axis-aligned rectangle with opposite corners `a` and `b`, counter-clockwise. */
export function rectangle(a: Point, b: Point): Poly {
  const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  return [
    [x0, y0],
    [x1, y0],
    [x1, y1],
    [x0, y1],
  ];
}

export function edgeLength(p: readonly Point[], i: number): number {
  const a = p[i];
  const b = p[(i + 1) % p.length];
  return a && b ? Math.hypot(b[0] - a[0], b[1] - a[1]) : 0;
}

/**
 * Sets the length of edge `i` by stretching the polygon along that edge: every vertex at
 * or beyond the edge's end (along its direction) moves. A rectangle stays a rectangle, an
 * L keeps its right angles.
 */
export function setEdgeLength(p: readonly Point[], i: number, length: number): Poly {
  const a = p[i];
  const b = p[(i + 1) % p.length];
  if (!a || !b) return [...p];
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  if (len === 0) return [...p];
  const d: Point = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
  const delta = length - len;
  const end = b[0] * d[0] + b[1] * d[1];
  const eps = 1e-9 * Math.max(1, Math.abs(end));
  return p.map(([x, y]) =>
    x * d[0] + y * d[1] >= end - eps ? [x + d[0] * delta, y + d[1] * delta] : [x, y],
  );
}

/** Nearest vertex within `tol` (m), or -1. */
export function nearestVertex(p: readonly Point[], at: Point, tol: number): number {
  let best = -1;
  let bestD = tol;
  p.forEach(([x, y], i) => {
    const d = Math.hypot(x - at[0], y - at[1]);
    if (d <= bestD) {
      best = i;
      bestD = d;
    }
  });
  return best;
}

/** Projection of `at` on segment `a → b`: parameter in [0, 1] and distance. */
export function projectOnSegment(
  a: Point,
  b: Point,
  at: Point,
): { t: number; point: Point; dist: number } {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const l2 = dx * dx + dy * dy;
  const t =
    l2 === 0 ? 0 : Math.min(1, Math.max(0, ((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / l2));
  const point: Point = [a[0] + dx * t, a[1] + dy * t];
  return { t, point, dist: Math.hypot(at[0] - point[0], at[1] - point[1]) };
}

/** Nearest edge within `tol` (m): its index and the projected point, or `null`. */
export function nearestEdge(
  p: readonly Point[],
  at: Point,
  tol: number,
): { edge: number; point: Point } | null {
  let best: { edge: number; point: Point } | null = null;
  let bestD = tol;
  p.forEach((a, i) => {
    const b = p[(i + 1) % p.length] ?? a;
    const pr = projectOnSegment(a, b, at);
    if (pr.dist <= bestD) {
      best = { edge: i, point: pr.point };
      bestD = pr.dist;
    }
  });
  return best;
}

/** Whether two segments cross (touching counts), for the polygon tool's closing check. */
function segmentsCross(a: Point, b: Point, c: Point, d: Point): boolean {
  const orient = (p: Point, q: Point, r: Point) =>
    Math.sign((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]));
  const o1 = orient(a, b, c);
  const o2 = orient(a, b, d);
  const o3 = orient(c, d, a);
  const o4 = orient(c, d, b);
  return o1 !== o2 && o3 !== o4;
}

/** Whether a closed polygon is simple (no two non-adjacent edges cross). */
export function isSimple(p: readonly Point[]): boolean {
  const n = p.length;
  if (n < 3) return false;
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      if (j === i + 1 || (i === 0 && j === n - 1)) continue;
      const a = p[i];
      const b = p[(i + 1) % n];
      const c = p[j];
      const d = p[(j + 1) % n];
      if (a && b && c && d && segmentsCross(a, b, c, d)) return false;
    }
  }
  return true;
}

/** Whether a counter-clockwise polygon is convex (no re-entrant corner). */
export function isConvex(p: readonly Point[]): boolean {
  const n = p.length;
  return p.every((b, i) => {
    const a = p[(i + n - 1) % n] ?? b;
    const c = p[(i + 1) % n] ?? b;
    return (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) >= -1e-9;
  });
}
