import {
  BufferGeometry,
  Color,
  ExtrudeGeometry,
  Float32BufferAttribute,
  Matrix4,
  Shape,
  ShapeGeometry,
  Vector2,
  Vector3,
} from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { ProjectSummary, RoofFace, RoofKind, WallSummary } from '../bindings';
import { roofMatrix } from './frames';

// Pitched roofs share the tile colour.
const PITCHED = new Color('#8d5a4a');
const ROOF_COLORS: Record<RoofKind, Color> = {
  gable: PITCHED,
  hipped: PITCHED,
  monopitch: PITCHED,
  flat: new Color('#8f969c'),
};

/** Roof-local `(u, v)` polygon of a face: its outline, or its rectangle. */
export function faceOutline(face: RoofFace): [number, number][] {
  if (face.outline) return face.outline.verticesUv.map(([u, v]) => [u, v]);
  return [
    [0, 0],
    [face.widthM, 0],
    [face.widthM, face.heightM],
    [0, face.heightM],
  ];
}

function shapeOf(points: [number, number][]): Shape {
  return new Shape(points.map(([x, y]) => new Vector2(x, y)));
}

/**
 * One wall: the vertical polygon on footprint edge `start → end`, from its foot up to the
 * roof profile computed in Rust. Built in the wall's `(s, z)` plane, then placed.
 */
export function wallGeometry(wall: WallSummary): BufferGeometry | null {
  const [x0, y0] = wall.start;
  const [x1, y1] = wall.end;
  const len = Math.hypot(x1 - x0, y1 - y0);
  if (len === 0 || wall.profile.length < 2) return null;
  const points: [number, number][] = [
    [0, wall.baseZ],
    [len, wall.baseZ],
    ...[...wall.profile].reverse().map(([s, z]): [number, number] => [s, z]),
  ];
  const geometry = new ShapeGeometry(shapeOf(points));
  const dir = new Vector3((x1 - x0) / len, (y1 - y0) / len, 0);
  const up = new Vector3(0, 0, 1);
  const normal = new Vector3().crossVectors(dir, up);
  geometry.applyMatrix4(new Matrix4().makeBasis(dir, up, normal).setPosition(x0, y0, 0));
  return geometry;
}

export function wallsGeometry(summary: ProjectSummary): BufferGeometry | null {
  const parts = summary.walls.map(wallGeometry).filter((g): g is BufferGeometry => g !== null);
  return parts.length ? mergeGeometries(parts) : null;
}

/** Roof faces, triangulated from their real outline (triangles, trapezoids, L shapes). */
export function roofsGeometry(summary: ProjectSummary): BufferGeometry | null {
  const kindOf = new Map(summary.buildings.map((b) => [b.id, b.roofKind]));
  const parts = summary.roofFaces.map(({ face, frame }) => {
    const geometry = new ShapeGeometry(shapeOf(faceOutline(face)));
    geometry.applyMatrix4(roofMatrix(frame));
    const color = ROOF_COLORS[kindOf.get(face.buildingId) ?? 'gable'];
    const count = geometry.getAttribute('position').count;
    const colors = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) color.toArray(colors, i * 3);
    geometry.setAttribute('color', new Float32BufferAttribute(colors, 3));
    return geometry;
  });
  return parts.length ? mergeGeometries(parts) : null;
}

/** Thickness drawn for obstacles without height (areas kept free), m. */
const FLAT_OBSTACLE_M = 0.02;

/**
 * Roof obstacles (chimneys, windows…): their footprint on the face extruded along the
 * face's outward normal by their height.
 */
export function obstaclesGeometry(summary: ProjectSummary): BufferGeometry | null {
  const parts: BufferGeometry[] = [];
  for (const { face, frame } of summary.roofFaces) {
    for (const o of face.outline?.obstacles ?? []) {
      if (o.footprintUv.length < 3) continue;
      const geometry = new ExtrudeGeometry(
        shapeOf(o.footprintUv.map(([u, v]): [number, number] => [u, v])),
        {
          depth: Math.max(o.heightM, FLAT_OBSTACLE_M),
          bevelEnabled: false,
        },
      );
      geometry.deleteAttribute('uv');
      geometry.applyMatrix4(roofMatrix(frame));
      parts.push(geometry);
    }
  }
  return parts.length ? mergeGeometries(parts) : null;
}
