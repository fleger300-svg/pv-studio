import { BufferGeometry, Color, Float32BufferAttribute, ShapeUtils, Vector2, Vector3 } from 'three';
import type { ShadingObject, ShadingObjectKind } from '../bindings';

type P3 = [number, number, number];

const KIND_COLORS: Record<ShadingObjectKind, Color> = {
  building: new Color('#c9c2b6'),
  roofFixture: new Color('#5b5f66'),
  wall: new Color('#a8a196'),
  tree: new Color('#6b4f3a'), // trunk; crowns have their own material
  polygon: new Color('#9a8fb0'),
};

/** A tree's crown: partly transparent, drawn as the spheroid the shading uses. */
export const isCrown = (o: ShadingObject): boolean => (o.opacityByMonth?.length ?? 0) > 0;

/** Sides and rings of a crown's spheroid. */
const CROWN_SIDES = 16;
const CROWN_RINGS = 8;

/**
 * Indices of the triangles of a planar polygon (any orientation), from its projection on
 * the plane most facing its normal (Newell's normal).
 */
function triangulate(points: readonly P3[]): number[] {
  let nx = 0;
  let ny = 0;
  let nz = 0;
  for (let i = 0; i < points.length; i++) {
    const [x0, y0, z0] = points[i] as P3;
    const [x1, y1, z1] = points[(i + 1) % points.length] as P3;
    nx += (y0 - y1) * (z0 + z1);
    ny += (z0 - z1) * (x0 + x1);
    nz += (x0 - x1) * (y0 + y1);
  }
  const ax = Math.abs(nx);
  const ay = Math.abs(ny);
  const az = Math.abs(nz);
  const flat: Vector2[] = points.map(([x, y, z]) =>
    az >= ax && az >= ay ? new Vector2(x, y) : ax >= ay ? new Vector2(y, z) : new Vector2(x, z),
  );
  return ShapeUtils.triangulateShape(flat, []).flat();
}

/** Non-indexed triangles with one colour per object (flat-shaded). */
class Soup {
  positions: number[] = [];
  colors: number[] = [];

  tri(a: P3, b: P3, c: P3, color: Color): void {
    this.positions.push(...a, ...b, ...c);
    for (let k = 0; k < 3; k++) this.colors.push(color.r, color.g, color.b);
  }

  geometry(): BufferGeometry | null {
    if (this.positions.length === 0) return null;
    const g = new BufferGeometry();
    g.setAttribute('position', new Float32BufferAttribute(this.positions, 3));
    g.setAttribute('color', new Float32BufferAttribute(this.colors, 3));
    g.computeVertexNormals();
    return g;
  }
}

/** A solid: the outline extruded upwards by its height, closed by its base and top. */
function solid(o: ShadingObject, color: Color, out: Soup): void {
  const base = o.outline as P3[];
  const top = base.map(([x, y, z]): P3 => [x, y, z + o.heightM]);
  const n = base.length;
  for (let i = 0; i < n; i++) {
    const j = (i + 1) % n;
    const [bi, bj, ti, tj] = [base[i], base[j], top[i], top[j]] as P3[];
    out.tri(bi as P3, bj as P3, tj as P3, color);
    out.tri(bi as P3, tj as P3, ti as P3, color);
  }
  if (n >= 3) {
    const idx = triangulate(base);
    for (let k = 0; k + 2 < idx.length; k += 3) {
      const [a, b, c] = [idx[k], idx[k + 1], idx[k + 2]] as [number, number, number];
      out.tri(base[a] as P3, base[b] as P3, base[c] as P3, color);
      out.tri(top[a] as P3, top[b] as P3, top[c] as P3, color);
    }
  }
}

/** A flat polygon drawn in 3-D. */
function polygon(o: ShadingObject, color: Color, out: Soup): void {
  const pts = o.outline as P3[];
  const idx = triangulate(pts);
  for (let k = 0; k + 2 < idx.length; k += 3) {
    out.tri(
      pts[idx[k] as number] as P3,
      pts[idx[k + 1] as number] as P3,
      pts[idx[k + 2] as number] as P3,
      color,
    );
  }
}

/** A crown: the spheroid inscribed in the extruded outline (as in the shading model). */
function crown(o: ShadingObject, color: Color, out: Soup): void {
  const base = o.outline as P3[];
  const n = base.length;
  const c = new Vector3();
  for (const [x, y, z] of base) c.add(new Vector3(x, y, z));
  c.divideScalar(n);
  const r = base.reduce((s, [x, y]) => s + Math.hypot(x - c.x, y - c.y), 0) / n;
  const half = o.heightM / 2;
  const at = (ring: number, side: number): P3 => {
    const phi = (Math.PI * ring) / CROWN_RINGS; // 0 = top
    const a = (2 * Math.PI * side) / CROWN_SIDES;
    const rr = r * Math.sin(phi);
    return [c.x + rr * Math.cos(a), c.y + rr * Math.sin(a), c.z + half - half * Math.cos(phi)];
  };
  for (let k = 0; k < CROWN_RINGS; k++) {
    for (let s = 0; s < CROWN_SIDES; s++) {
      const t = s + 1;
      if (k > 0) out.tri(at(k, s), at(k + 1, s), at(k, t), color);
      if (k < CROWN_RINGS - 1) out.tri(at(k, t), at(k + 1, s), at(k + 1, t), color);
    }
  }
}

/** Crown colour. */
const CROWN = new Color('#4f7d3a');

/**
 * Shading objects for the 3-D view: opaque solids and polygons (one colour per kind), and
 * the trees' crowns apart (drawn semi-transparent).
 */
export function objectsGeometry(objects: readonly ShadingObject[]): {
  solids: BufferGeometry | null;
  crowns: BufferGeometry | null;
} {
  const solids = new Soup();
  const crowns = new Soup();
  for (const o of objects) {
    if (isCrown(o)) {
      if (o.outline.length >= 3 && o.heightM > 0) crown(o, CROWN, crowns);
    } else if (o.heightM > 0) {
      solid(o, KIND_COLORS[o.kind], solids);
    } else if (o.outline.length >= 3) {
      polygon(o, KIND_COLORS[o.kind], solids);
    }
  }
  return { solids: solids.geometry(), crowns: crowns.geometry() };
}
