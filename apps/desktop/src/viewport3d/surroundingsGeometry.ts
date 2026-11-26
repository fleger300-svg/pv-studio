import { BufferGeometry, Float32BufferAttribute, Uint32BufferAttribute } from 'three';
import type { SurroundingsView } from '../bindings';

/**
 * Height field of the surroundings (terrain, neighbouring buildings of the surface model):
 * one vertex per cell centre at its height, two triangles per square of four cells that all
 * shade (none over the project's own buildings). `null` without any such square. Pure.
 */
export function surroundingsGeometry(v: SurroundingsView): BufferGeometry | null {
  const { cols, rows, cellM } = v;
  const positions = new Float32Array(cols * rows * 3);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const i = r * cols + c;
      positions[i * 3] = v.originEastM + (c + 0.5) * cellM;
      positions[i * 3 + 1] = v.originNorthM - (r + 0.5) * cellM;
      positions[i * 3 + 2] = (v.heightsCm[i] ?? 0) / 100;
    }
  }
  const has = (i: number) => v.heightsCm[i] != null;
  const index: number[] = [];
  for (let r = 0; r + 1 < rows; r++) {
    for (let c = 0; c + 1 < cols; c++) {
      const a = r * cols + c; // north-west
      const b = a + 1; // north-east
      const d = a + cols; // south-west
      const e = d + 1; // south-east
      if (has(a) && has(b) && has(d) && has(e)) {
        // Counter-clockwise seen from above (z up): normals point up.
        index.push(a, d, e, a, e, b);
      }
    }
  }
  if (index.length === 0) return null;
  const g = new BufferGeometry();
  g.setAttribute('position', new Float32BufferAttribute(positions, 3));
  g.setIndex(new Uint32BufferAttribute(index, 1));
  g.computeVertexNormals();
  return g;
}

/** Lowest height of the surroundings [m]; null without any. Pure. */
export function surroundingsFloor(v: SurroundingsView): number | null {
  let low = Infinity;
  for (const h of v.heightsCm) if (h != null && h < low) low = h;
  return Number.isFinite(low) ? low / 100 : null;
}
