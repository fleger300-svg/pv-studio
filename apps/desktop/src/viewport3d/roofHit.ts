import type { RoofFaceSummary } from '../bindings';

export type V3 = readonly [number, number, number];

const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

export interface RoofHit {
  faceIndex: number;
  u: number;
  v: number;
  /** Ray parameter (distance along a unit ray). */
  t: number;
}

/** Intersection of a ray with the (infinite) plane of one roof face, in roof `(u, v)`. */
export function planeUV(
  face: RoofFaceSummary,
  origin: V3,
  dir: V3,
): { u: number; v: number; t: number } | null {
  const { frame } = face;
  const denom = dot(dir, frame.normal);
  if (Math.abs(denom) < 1e-9) return null;
  const toPlane: V3 = [
    frame.origin[0] - origin[0],
    frame.origin[1] - origin[1],
    frame.origin[2] - origin[2],
  ];
  const t = dot(toPlane, frame.normal) / denom;
  if (t <= 0) return null;
  const d: V3 = [
    origin[0] + dir[0] * t - frame.origin[0],
    origin[1] + dir[1] * t - frame.origin[1],
    origin[2] + dir[2] * t - frame.origin[2],
  ];
  return { u: dot(d, frame.axisU), v: dot(d, frame.axisV), t };
}

/**
 * Closest roof face hit by the ray, without any per-panel raycasting: only the roof
 * rectangles are tested (a handful of planes, whatever the module count).
 */
export function hitRoofs(faces: readonly RoofFaceSummary[], origin: V3, dir: V3): RoofHit | null {
  let best: RoofHit | null = null;
  faces.forEach((face, faceIndex) => {
    const hit = planeUV(face, origin, dir);
    if (!hit) return;
    const { u, v, t } = hit;
    if (u < 0 || v < 0 || u > face.face.widthM || v > face.face.heightM) return;
    if (!best || t < best.t) best = { faceIndex, u, v, t };
  });
  return best;
}
