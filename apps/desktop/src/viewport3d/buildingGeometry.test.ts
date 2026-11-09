import { Vector3 } from 'three';
import { describe, expect, it } from 'vitest';
import type { BufferGeometry } from 'three';
import { roofsGeometry, wallGeometry } from './buildingGeometry';
import { flatRoofFixture } from './testSummary';

function triangles(g: BufferGeometry): Vector3[][] {
  const pos = g.getAttribute('position');
  const index = g.getIndex();
  const at = (i: number) => new Vector3(pos.getX(i), pos.getY(i), pos.getZ(i));
  const out: Vector3[][] = [];
  const n = index ? index.count : pos.count;
  for (let k = 0; k < n; k += 3) {
    const ids = [0, 1, 2].map((j) => (index ? index.getX(k + j) : k + j));
    out.push(ids.map(at));
  }
  return out;
}

function area(tris: Vector3[][]): number {
  return tris.reduce((sum, [a, b, c]) => {
    if (!a || !b || !c) return sum;
    return sum + new Vector3().crossVectors(b.clone().sub(a), c.clone().sub(a)).length() / 2;
  }, 0);
}

describe('wallGeometry', () => {
  it('builds a gable wall up to its roof profile, in its vertical plane', () => {
    const g = wallGeometry({
      buildingId: 'b',
      start: [10, 5],
      end: [10, 15],
      baseZ: 0,
      profile: [
        [0, 6],
        [5, 9],
        [10, 6],
      ],
    });
    expect(g).not.toBeNull();
    const tris = triangles(g as BufferGeometry);
    // 10 × 6 rectangle + gable triangle 10 × 3 / 2.
    expect(area(tris)).toBeCloseTo(75, 9);
    const pts = tris.flat();
    expect(pts.every((p) => Math.abs(p.x - 10) < 1e-9)).toBe(true);
    expect(Math.max(...pts.map((p) => p.z))).toBeCloseTo(9, 9);
    expect(Math.min(...pts.map((p) => p.y))).toBeCloseTo(5, 9);
  });

  it('skips degenerate walls', () => {
    expect(
      wallGeometry({ buildingId: 'b', start: [0, 0], end: [0, 0], baseZ: 0, profile: [] }),
    ).toBe(null);
  });
});

describe('roofsGeometry', () => {
  it('triangulates a triangular hip end from its outline', () => {
    const { summary } = flatRoofFixture();
    const face = summary.roofFaces[0];
    if (!face) throw new Error('fixture');
    face.face = {
      ...face.face,
      outline: {
        verticesUv: [
          [0, 0],
          [8, 0],
          [4, 3],
        ],
        edgeKinds: ['eave', 'hip', 'hip'],
        obstacles: [],
      },
    };
    const g = roofsGeometry(summary);
    expect(g).not.toBeNull();
    const tris = triangles(g as BufferGeometry);
    expect(tris).toHaveLength(1);
    expect(area(tris)).toBeCloseTo(12, 9);
    // Placed by the face frame (flat roof at z = 3).
    expect(tris.flat().every((p) => Math.abs(p.z - 3) < 1e-9)).toBe(true);
  });
});
