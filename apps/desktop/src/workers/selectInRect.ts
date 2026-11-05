/** Rectangle in normalised device coordinates (−1 … 1), any corner order. */
export interface NdcRect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/**
 * Ids whose projected centre lies inside `rect`. `viewProj` is a column-major 4 × 4 matrix
 * (three.js `Matrix4.elements` of `projection × view`). `skip` excludes ids (e.g. removed).
 */
export function selectInRect(
  positions: Float32Array,
  count: number,
  viewProj: ArrayLike<number>,
  rect: NdcRect,
  skip?: (id: number) => boolean,
): Uint32Array {
  const x0 = Math.min(rect.x0, rect.x1);
  const x1 = Math.max(rect.x0, rect.x1);
  const y0 = Math.min(rect.y0, rect.y1);
  const y1 = Math.max(rect.y0, rect.y1);
  const e = Array.from({ length: 16 }, (_, i) => viewProj[i] ?? 0) as [
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
  ];
  const out = new Uint32Array(count);
  let n = 0;
  for (let i = 0; i < count; i++) {
    const x = positions[i * 3] ?? 0;
    const y = positions[i * 3 + 1] ?? 0;
    const z = positions[i * 3 + 2] ?? 0;
    const w = e[3] * x + e[7] * y + e[11] * z + e[15];
    if (w <= 0) continue; // behind the camera
    const nx = (e[0] * x + e[4] * y + e[8] * z + e[12]) / w;
    if (nx < x0 || nx > x1) continue;
    const ny = (e[1] * x + e[5] * y + e[9] * z + e[13]) / w;
    if (ny < y0 || ny > y1) continue;
    const nz = (e[2] * x + e[6] * y + e[10] * z + e[14]) / w;
    if (nz < -1 || nz > 1) continue;
    if (skip?.(i)) continue;
    out[n++] = i;
  }
  return out.slice(0, n);
}
