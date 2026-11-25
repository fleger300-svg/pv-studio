import { describe, expect, it } from 'vitest';
import type { BufferGeometry } from 'three';
import type { ShadingObject } from '../bindings';
import { objectsGeometry } from './objectGeometry';

function bounds(g: BufferGeometry) {
  g.computeBoundingBox();
  const b = g.boundingBox;
  if (!b) throw new Error('no bounds');
  return b;
}

const square = (x: number, y: number, z: number, s: number): [number, number, number][] => [
  [x, y, z],
  [x + s, y, z],
  [x + s, y + s, z],
  [x, y + s, z],
];

describe('objectsGeometry', () => {
  it('extrudes a solid and closes it', () => {
    const o: ShadingObject = {
      name: 'chimney',
      kind: 'roofFixture',
      outline: square(2, 3, 5, 1),
      heightM: 1.5,
      opacityByMonth: [],
    };
    const { solids, crowns } = objectsGeometry([o]);
    expect(crowns).toBeNull();
    const g = solids as BufferGeometry;
    // 4 walls × 2 + base 2 + top 2 triangles.
    expect(g.getAttribute('position').count).toBe(12 * 3);
    const b = bounds(g);
    expect(b.min.toArray()).toEqual([2, 3, 5]);
    expect(b.max.toArray()).toEqual([3, 4, 6.5]);
  });

  it('triangulates a vertical 3-D polygon', () => {
    const o: ShadingObject = {
      name: 'screen',
      kind: 'polygon',
      outline: [
        [0, 0, 0],
        [4, 0, 0],
        [4, 0, 2],
        [0, 0, 2],
      ],
      heightM: 0,
      opacityByMonth: [],
    };
    const g = objectsGeometry([o]).solids as BufferGeometry;
    expect(g.getAttribute('position').count).toBe(2 * 3);
  });

  it('draws a crown as the spheroid inscribed in its outline, apart from the trunk', () => {
    const ring = Array.from({ length: 8 }, (_, k): [number, number, number] => {
      const a = (2 * Math.PI * k) / 8;
      return [10 + 3 * Math.cos(a), 20 + 3 * Math.sin(a), 4];
    });
    const crown: ShadingObject = {
      name: 'oak',
      kind: 'tree',
      outline: ring,
      heightM: 6,
      opacityByMonth: Array<number>(12).fill(0.6),
    };
    const trunk: ShadingObject = {
      name: 'oak',
      kind: 'tree',
      outline: square(9.8, 19.8, 0, 0.4),
      heightM: 4,
      opacityByMonth: [],
    };
    const { solids, crowns } = objectsGeometry([crown, trunk]);
    expect(solids).not.toBeNull();
    const b = bounds(crowns as BufferGeometry);
    expect(b.min.z).toBeCloseTo(4, 9);
    expect(b.max.z).toBeCloseTo(10, 9);
    expect(b.max.x).toBeCloseTo(13, 5);
    expect(b.min.y).toBeCloseTo(17, 1);
  });

  it('returns nothing without objects', () => {
    expect(objectsGeometry([])).toEqual({ solids: null, crowns: null });
  });
});
