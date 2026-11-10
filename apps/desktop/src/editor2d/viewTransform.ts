/**
 * Plan view transform: world ENU metres (east, north) ↔ screen pixels, north up.
 * `x`, `y` are the screen position of the world origin, `scale` is pixels per metre.
 */
export interface ViewTransform {
  scale: number;
  x: number;
  y: number;
}

export type Point = readonly [number, number];

export interface Bounds {
  minE: number;
  minN: number;
  maxE: number;
  maxN: number;
}

/** Zoom limits: a 2 km site to a 1 cm detail on a usual screen. */
export const MIN_SCALE = 0.2;
export const MAX_SCALE = 2000;

export function toScreen(t: ViewTransform, [e, n]: Point): [number, number] {
  return [t.x + e * t.scale, t.y - n * t.scale];
}

export function toWorld(t: ViewTransform, [sx, sy]: Point): [number, number] {
  return [(sx - t.x) / t.scale, (t.y - sy) / t.scale];
}

/** Zooms by `factor` keeping the world point under the screen point `at` fixed. */
export function zoomAt(t: ViewTransform, at: Point, factor: number): ViewTransform {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, t.scale * factor));
  const [e, n] = toWorld(t, at);
  return { scale, x: at[0] - e * scale, y: at[1] + n * scale };
}

export function pan(t: ViewTransform, dx: number, dy: number): ViewTransform {
  return { ...t, x: t.x + dx, y: t.y + dy };
}

/** The transform that shows `bounds` centred in a `width × height` view with a margin. */
export function fit(bounds: Bounds, width: number, height: number, marginPx = 40): ViewTransform {
  const w = Math.max(bounds.maxE - bounds.minE, 1);
  const h = Math.max(bounds.maxN - bounds.minN, 1);
  const scale = Math.min(
    MAX_SCALE,
    Math.max(MIN_SCALE, Math.min((width - 2 * marginPx) / w, (height - 2 * marginPx) / h)),
  );
  const ce = (bounds.minE + bounds.maxE) / 2;
  const cn = (bounds.minN + bounds.maxN) / 2;
  return { scale, x: width / 2 - ce * scale, y: height / 2 + cn * scale };
}

const GRID_STEPS = [0.1, 0.5, 1, 5, 10, 50, 100, 500] as const;

/** Grid spacing (m): the finest step at least `minPx` pixels apart. */
export function gridStep(scale: number, minPx = 24): number {
  return GRID_STEPS.find((s) => s * scale >= minPx) ?? 500;
}

/** World rectangle visible in a `width × height` view. */
export function visibleBounds(t: ViewTransform, width: number, height: number): Bounds {
  const [minE, maxN] = toWorld(t, [0, 0]);
  const [maxE, minN] = toWorld(t, [width, height]);
  return { minE, minN, maxE, maxN };
}
