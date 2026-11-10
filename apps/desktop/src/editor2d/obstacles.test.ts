import { describe, expect, it } from 'vitest';
import type { RoofFace } from '../bindings';
import { boxOf, footprintOf, newObstacle, outlineOf, withObstacles } from './obstacles';

const face: RoofFace = {
  id: 'f',
  buildingId: 'b',
  name: 'South',
  origin: [0, 0, 6],
  tiltDeg: 30,
  azimuthDeg: 180,
  widthM: 10,
  heightM: 6,
};

describe('obstacles', () => {
  it('round-trips a box through its rectangle', () => {
    const box = { u: 3, v: 2, width: 1, length: 0.5 };
    expect(boxOf(footprintOf(box))).toEqual(box);
  });

  it('makes the default outline explicit to carry obstacles', () => {
    expect(outlineOf(face).edgeKinds).toEqual(['eave', 'verge', 'ridge', 'verge']);
    expect(outlineOf({ ...face, tiltDeg: 0 }).edgeKinds).toEqual([
      'parapet',
      'parapet',
      'parapet',
      'parapet',
    ]);
    const o = newObstacle(face, 'chimney', 'Chimney 1');
    const box = boxOf(o.footprintUv);
    expect([box.u, box.v]).toEqual([5, 3]);
    expect(box.width).toBeCloseTo(0.8, 12);
    expect(box.length).toBeCloseTo(0.8, 12);
    const f = withObstacles(face, [o]);
    expect(f.outline?.verticesUv).toHaveLength(4);
    expect(f.outline?.obstacles).toEqual([o]);
  });
});
