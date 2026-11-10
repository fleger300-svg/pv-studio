import { describe, expect, it } from 'vitest';
import { snap, snapStep } from './snapping';
import type { Point } from './viewTransform';

// 20 px/m: tolerance 8 px = 0.4 m; grid 5 m → snap step 0.5 m.
const scale = 20;
const vertices: Point[] = [
  [0, 0],
  [10, 0],
  [10, 6],
];

describe('snap', () => {
  it('uses a tenth of the visible grid', () => {
    expect(snapStep(20)).toBe(0.5);
    expect(snapStep(300)).toBe(0.01);
  });

  it('prefers a nearby vertex', () => {
    expect(snap({ point: [9.8, 5.75], scale, vertices })).toEqual({
      point: [10, 6],
      guides: [],
      kind: 'vertex',
    });
  });

  it('aligns with vertices, axis by axis, and shows guides', () => {
    const r = snap({ point: [10.3, 3.1], scale, vertices });
    expect(r.kind).toBe('align');
    expect(r.point).toEqual([10, 3]);
    expect(r.guides).toHaveLength(1);
    const both = snap({ point: [0.2, 5.8], scale, vertices });
    expect(both.point).toEqual([0, 6]);
    expect(both.guides).toHaveLength(2);
  });

  it('locks 45° multiples and edge directions from the anchor', () => {
    const r = snap({ point: [3.1, 3.3], scale, vertices: [], anchor: [0, 0] });
    expect(r.kind).toBe('angle');
    expect(r.point[0]).toBeCloseTo(r.point[1], 12);
    // A rotated building: its edge direction (30°) is offered.
    const a = Math.PI / 6;
    const p: Point = [4 * Math.cos(a) + 0.1, 4 * Math.sin(a) - 0.05];
    const rot = snap({ point: p, scale, vertices: [], anchor: [0, 0], directions: [a] });
    expect(rot.kind).toBe('angle');
    expect(Math.atan2(rot.point[1], rot.point[0])).toBeCloseTo(a, 12);
  });

  it('falls back to the grid, and Alt disables everything', () => {
    expect(snap({ point: [3.26, 2.74], scale, vertices: [] })).toEqual({
      point: [3.5, 2.5],
      guides: [],
      kind: 'grid',
    });
    expect(snap({ point: [3.26, 2.74], scale, vertices, disabled: true }).point).toEqual([
      3.26, 2.74,
    ]);
  });
});
