import { Box3, Matrix4, Vector3 } from 'three';
import type { ProjectSummary, RoofFrame } from '../bindings';
import type { ModuleData } from './moduleBuffer';

export const vec3 = (a: readonly [number, number, number]): Vector3 =>
  new Vector3(a[0], a[1], a[2]);

/** Matrix mapping roof-local (u, v, w) to world ENU, from the Rust-computed frame. */
export function roofMatrix(frame: RoofFrame): Matrix4 {
  return new Matrix4()
    .makeBasis(vec3(frame.axisU), vec3(frame.axisV), vec3(frame.normal))
    .setPosition(vec3(frame.origin));
}

/** World-space displacement of a roof-local `(du, dv)` move. */
export function roofDelta(frame: RoofFrame, du: number, dv: number, out = new Vector3()): Vector3 {
  return out
    .set(0, 0, 0)
    .addScaledVector(vec3(frame.axisU), du)
    .addScaledVector(vec3(frame.axisV), dv);
}

/** Bounding box of buildings and modules (world ENU), always including the ground origin. */
export function sceneBounds(summary: ProjectSummary, modules: ModuleData): Box3 {
  const box = new Box3();
  const p = new Vector3();
  for (let i = 0; i < modules.count; i++) {
    box.expandByPoint(
      p.set(
        modules.positions[i * 3] ?? 0,
        modules.positions[i * 3 + 1] ?? 0,
        modules.positions[i * 3 + 2] ?? 0,
      ),
    );
  }
  for (const b of summary.buildings) {
    const [x, y, z] = b.origin;
    for (const [dx, dy] of b.footprint) {
      box.expandByPoint(p.set(x + dx, y + dy, z));
      box.expandByPoint(p.set(x + dx, y + dy, z + b.ridgeHeightM));
    }
  }
  if (box.isEmpty()) box.set(new Vector3(-10, -10, 0), new Vector3(10, 10, 5));
  return box;
}
