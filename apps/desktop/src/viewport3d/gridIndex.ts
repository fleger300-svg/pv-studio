import type { ProjectSummary } from '../bindings';
import { FLAG_REMOVED, type ModuleData } from './moduleBuffer';

const OFFSET = 32768;
const key = (row: number, col: number) => (row + OFFSET) * 65536 + (col + OFFSET);

/**
 * Click lookup by grid: per array, modules are binned by their *current* roof `(u, v)` on the
 * array's row/column pitch. A hit tests only the 3 × 3 neighbouring cells against the module
 * footprints — O(1), no per-panel raycasting, and still correct after moves.
 */
export class GridIndex {
  private readonly bins: Map<number, number[]>[];
  private readonly keys: Float64Array;
  /** Current roof (u, v) of each module. */
  readonly uv: Float64Array;

  constructor(
    private readonly summary: ProjectSummary,
    private readonly data: ModuleData,
  ) {
    this.bins = summary.arrays.map(() => new Map<number, number[]>());
    this.keys = new Float64Array(data.count);
    this.uv = new Float64Array(data.count * 2);
    for (let id = 0; id < data.count; id++) this.insert(id);
  }

  private computeUV(id: number): void {
    const a = this.summary.arrays[this.data.arrayIndex[id] ?? 0];
    const face = a && this.summary.roofFaces[a.roofFaceIndex];
    if (!face) return;
    const { origin, axisU, axisV } = face.frame;
    const p = this.data.positions;
    const dx = (p[id * 3] ?? 0) - origin[0];
    const dy = (p[id * 3 + 1] ?? 0) - origin[1];
    const dz = (p[id * 3 + 2] ?? 0) - origin[2];
    this.uv[id * 2] = dx * axisU[0] + dy * axisU[1] + dz * axisU[2];
    this.uv[id * 2 + 1] = dx * axisV[0] + dy * axisV[1] + dz * axisV[2];
  }

  // TODO(pv-layout blocks): this assumes evenly spaced cells (`origin + index × pitch`).
  // Arrays laid out with mounting blocks (`grid.blockCols/blockRows > 0`) add
  // `blockExtraU/V` between blocks, so the index computed here drifts by that much per block
  // and the ±1 neighbourhood searched in `hit()` stops covering the right cell after a few
  // blocks. Migrate to the block-aware formula of Rust `ArrayGrid::cell_centre()` (and its
  // inverse) when the visual interface is resumed. No impact today: no array has blocks yet.
  private cell(arrayIndex: number, u: number, v: number): number {
    const g = this.summary.arrays[arrayIndex]?.array.grid;
    if (!g) return 0;
    return key(Math.round((v - g.originV) / g.pitchV), Math.round((u - g.originU) / g.pitchU));
  }

  private insert(id: number): void {
    this.computeUV(id);
    const ai = this.data.arrayIndex[id] ?? 0;
    const k = this.cell(ai, this.uv[id * 2] ?? 0, this.uv[id * 2 + 1] ?? 0);
    this.keys[id] = k;
    const bins = this.bins[ai];
    if (!bins) return;
    let bin = bins.get(k);
    if (!bin) bins.set(k, (bin = []));
    bin.push(id);
  }

  /** Re-bins modules whose position changed (after a move, undo or redo). */
  update(ids: Iterable<number>): void {
    for (const id of ids) {
      const bin = this.bins[this.data.arrayIndex[id] ?? 0]?.get(this.keys[id] ?? 0);
      const at = bin?.indexOf(id) ?? -1;
      if (bin && at >= 0) bin.splice(at, 1);
      this.insert(id);
    }
  }

  /** Active module under roof point `(u, v)` of face `faceIndex`, or `null`. */
  hit(faceIndex: number, u: number, v: number): number | null {
    let best: number | null = null;
    let bestDist = Infinity;
    this.summary.arrays.forEach((a, ai) => {
      if (a.roofFaceIndex !== faceIndex) return;
      const [hu, hv] = a.halfFootprint;
      const g = a.array.grid;
      // TODO(pv-layout blocks): same evenly-spaced assumption as `cell()`; use the
      // block-aware `ArrayGrid::cell_centre()` formula (see the note on `cell()`).
      const row = Math.round((v - g.originV) / g.pitchV);
      const col = Math.round((u - g.originU) / g.pitchU);
      for (let dr = -1; dr <= 1; dr++) {
        for (let dc = -1; dc <= 1; dc++) {
          for (const id of this.bins[ai]?.get(key(row + dr, col + dc)) ?? []) {
            if ((this.data.flags[id] ?? 0) & FLAG_REMOVED) continue;
            const du = Math.abs(u - (this.uv[id * 2] ?? 0));
            const dv = Math.abs(v - (this.uv[id * 2 + 1] ?? 0));
            if (du > hu || dv > hv) continue;
            const dist = du * du + dv * dv;
            if (dist < bestDist) {
              best = id;
              bestDist = dist;
            }
          }
        }
      }
    });
    return best;
  }
}
