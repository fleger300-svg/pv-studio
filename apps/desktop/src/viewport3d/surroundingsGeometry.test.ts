import { describe, expect, it } from 'vitest';
import { Vector3 } from 'three';
import { surroundingsFloor, surroundingsGeometry } from './surroundingsGeometry';

/** The value, failing the test when it is missing. */
function must<T>(x: T | null | undefined): T {
  if (x == null) throw new Error('missing');
  return x;
}

const view = (heightsCm: (number | null)[], cols: number, rows: number) => ({
  originEastM: 10,
  originNorthM: 20,
  cellM: 2,
  cols,
  rows,
  heightsCm,
});

describe('surroundingsGeometry', () => {
  it('lays a vertex on each cell centre at its height, facing up', () => {
    const g = must(surroundingsGeometry(view([100, 200, 300, 400], 2, 2)));
    const p = g.getAttribute('position');
    // Cell (row 1, col 0): centre (11, 17), 3 m.
    expect([p.getX(2), p.getY(2), p.getZ(2)]).toEqual([11, 17, 3]);
    expect(must(g.getIndex()).count).toBe(6);
    const flat = must(surroundingsGeometry(view([0, 0, 0, 0], 2, 2)));
    const n = new Vector3().fromBufferAttribute(flat.getAttribute('normal'), 0);
    expect(n.z).toBeCloseTo(1);
  });

  it('leaves the squares touching a hole out', () => {
    // 3 × 2 cells, the middle top one a hole: no square left on either side.
    expect(surroundingsGeometry(view([0, null, 0, 0, 0, 0], 3, 2))).toBeNull();
    // 3 × 2, a corner hole: the other square stays.
    expect(
      must(must(surroundingsGeometry(view([null, 0, 0, 0, 0, 0], 3, 2))).getIndex()).count,
    ).toBe(6);
  });

  it('finds the lowest cell', () => {
    expect(surroundingsFloor(view([250, null, -120, 30], 2, 2))).toBe(-1.2);
    expect(surroundingsFloor(view([null], 1, 1))).toBeNull();
  });
});
