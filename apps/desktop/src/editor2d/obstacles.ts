import type { ObstacleKind, RoofFace, RoofObstacle, RoofOutlineSpec } from '../bindings';
import { RECT_FLAT, RECT_PITCHED } from './planGeometry';

/** An obstacle as the form edits it: an axis-aligned rectangle on the face, in (u, v). */
export interface ObstacleBox {
  /** Centre, m from the face origin (u along the eave, v up the slope). */
  u: number;
  v: number;
  /** Size along u and v, m. */
  width: number;
  length: number;
}

/** The bounding box of an obstacle footprint (any polygon). */
export function boxOf(footprint: readonly (readonly [number, number])[]): ObstacleBox {
  const us = footprint.map((p) => p[0]);
  const vs = footprint.map((p) => p[1]);
  const [u0, u1, v0, v1] = [Math.min(...us), Math.max(...us), Math.min(...vs), Math.max(...vs)];
  return { u: (u0 + u1) / 2, v: (v0 + v1) / 2, width: u1 - u0, length: v1 - v0 };
}

/** The rectangle of a box, counter-clockwise. */
export function footprintOf(b: ObstacleBox): [number, number][] {
  const [hw, hl] = [b.width / 2, b.length / 2];
  return [
    [b.u - hw, b.v - hl],
    [b.u + hw, b.v - hl],
    [b.u + hw, b.v + hl],
    [b.u - hw, b.v + hl],
  ];
}

/** The face's outline, made explicit (its rectangle and default edge kinds) if it has none. */
export function outlineOf(face: RoofFace): RoofOutlineSpec {
  if (face.outline) return face.outline;
  return {
    verticesUv: [
      [0, 0],
      [face.widthM, 0],
      [face.widthM, face.heightM],
      [0, face.heightM],
    ],
    edgeKinds: face.tiltDeg < 5 ? [...RECT_FLAT] : [...RECT_PITCHED],
    obstacles: [],
  };
}

/** The face with its obstacles replaced (the outline made explicit if needed). */
export function withObstacles(face: RoofFace, obstacles: RoofObstacle[]): RoofFace {
  return { ...face, outline: { ...outlineOf(face), obstacles } };
}

/** Default size (m) and height (m) of a new obstacle of each kind. */
export const OBSTACLE_DEFAULTS: Record<
  ObstacleKind,
  { width: number; length: number; height: number }
> = {
  chimney: { width: 0.8, length: 0.8, height: 1.2 },
  skylight: { width: 0.8, length: 1.2, height: 0.15 },
  vent: { width: 0.3, length: 0.3, height: 0.4 },
  dormer: { width: 2.5, length: 2.5, height: 2 },
  keepOut: { width: 1, length: 1, height: 0 },
};

/** A new obstacle of `kind` at the centre of the face. */
export function newObstacle(face: RoofFace, kind: ObstacleKind, name: string): RoofObstacle {
  const d = OBSTACLE_DEFAULTS[kind];
  return {
    name,
    kind,
    footprintUv: footprintOf({
      u: face.widthM / 2,
      v: face.heightM / 2,
      width: d.width,
      length: d.length,
    }),
    heightM: d.height,
  };
}
