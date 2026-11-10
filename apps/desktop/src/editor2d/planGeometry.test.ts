import { describe, expect, it } from 'vitest';
import type { RoofFrame } from '../bindings';
import { flatRoofFixture } from '../viewport3d/testSummary';
import {
  centroid,
  contains,
  errorEdges,
  planBounds,
  planBuildings,
  planFaces,
  uvToPlan,
} from './planGeometry';

describe('planGeometry', () => {
  it('projects roof (u, v) on the ground plan', () => {
    // South face tilted 30°: u east, v up the slope (north, shortened by cos 30°).
    const c = Math.cos(Math.PI / 6);
    const frame: RoofFrame = {
      origin: [10, -5, 6],
      axisU: [1, 0, 0],
      axisV: [0, c, 0.5],
      normal: [0, -0.5, c],
    };
    const [e, n] = uvToPlan(frame, 2, 4);
    expect(e).toBeCloseTo(12, 12);
    expect(n).toBeCloseTo(-5 + 4 * c, 12);
  });

  it('builds faces with default edge kinds and bounds', () => {
    const { summary } = flatRoofFixture();
    const faces = planFaces(summary);
    expect(faces).toHaveLength(1);
    expect(faces[0]?.edges.map((e) => e.kind)).toEqual([
      'parapet',
      'parapet',
      'parapet',
      'parapet',
    ]);
    const b = planBounds(planBuildings(summary), faces);
    expect(b).not.toBeNull();
    expect(planBounds([], [])).toBeNull();
  });

  it('places buildings relative to their origin', () => {
    const { summary } = flatRoofFixture();
    summary.buildings = [
      {
        id: 'b',
        name: 'B',
        origin: [100, 50, 0],
        footprint: [
          [-5, -3],
          [5, -3],
          [5, 3],
          [-5, 3],
        ],
        sizeM: [10, 6],
        eaveHeightM: 6,
        ridgeHeightM: 6,
        roofKind: 'flat',
      },
    ];
    const [b] = planBuildings(summary);
    expect(b?.footprint[0]).toEqual([95, 47]);
    expect(centroid(b?.footprint ?? [])).toEqual([100, 50]);
    expect(contains(b?.footprint ?? [], [100, 50])).toBe(true);
    expect(contains(b?.footprint ?? [], [106, 50])).toBe(false);
  });

  it('highlights the edges of a refused re-entrant corner, else the whole outline', () => {
    expect([
      ...errorEdges({ key: 'errors.roof.reentrantCorner', params: { a: 3, b: 4 } }, 6),
    ]).toEqual([2, 3]);
    expect(
      errorEdges({ key: 'errors.entity.invalid', params: { field: 'footprint' } }, 4).size,
    ).toBe(4);
    expect(errorEdges(null, 4).size).toBe(0);
  });
});
