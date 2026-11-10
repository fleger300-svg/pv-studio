import { describe, expect, it } from 'vitest';
import { PolygonTool, RectTool, SelectTool, type ToolEnv } from './tools';
import type { Point } from './viewTransform';

// 20 px/m: snap step 0.5 m, tolerance 0.4 m.
const empty: ToolEnv = { scale: 20, buildings: [], selectedId: null };
const p = (world: Point, alt = false) => ({ world, alt });

const house = {
  id: 'h',
  name: 'H',
  footprint: [
    [0, 0],
    [10, 0],
    [10, 6],
    [0, 6],
  ] as Point[],
};
const withHouse: ToolEnv = { scale: 20, buildings: [house], selectedId: 'h' };

describe('RectTool', () => {
  it('drags a snapped rectangle and commits a new building', () => {
    const t = new RectTool();
    t.down(p([1.1, 0.9]), empty);
    t.move(p([8.2, 5.1]), empty);
    expect(t.view().draft?.footprint).toHaveLength(4);
    const r = t.up(p([8.2, 5.1]), empty);
    expect(r.commit).toEqual({
      buildingId: null,
      footprint: [
        [1, 1],
        [8, 1],
        [8, 5],
        [1, 5],
      ],
      changes: [],
      delta: null,
      open: false,
    });
  });

  it('ignores a click (smaller than 1 m)', () => {
    const t = new RectTool();
    t.down(p([1, 1]), empty);
    expect(t.up(p([1.2, 1.1]), empty).commit).toBeUndefined();
  });
});

describe('PolygonTool', () => {
  it('closes on the first vertex and orients the footprint counter-clockwise', () => {
    const t = new PolygonTool();
    // Clockwise L, by clicks (with a double-click duplicate ignored).
    for (const q of [
      [0, 0],
      [0, 20],
      [10, 20],
      [10, 10],
      [20, 10],
      [20, 10],
      [20, 0],
    ] as Point[]) {
      t.down(p(q), empty);
    }
    expect(t.view().draft?.open).toBe(true);
    const r = t.down(p([0.1, 0.2]), empty);
    expect(r.commit?.footprint).toHaveLength(6);
    const f = r.commit?.footprint ?? [];
    const area = f.reduce((s, [x0, y0], i) => {
      const [x1, y1] = f[(i + 1) % f.length] ?? [x0, y0];
      return s + x0 * y1 - x1 * y0;
    }, 0);
    expect(area).toBeGreaterThan(0);
  });

  it('refuses a self-intersecting outline, Backspace and Escape edit the stroke', () => {
    const t = new PolygonTool();
    for (const q of [
      [0, 0],
      [10, 10],
      [10, 0],
      [0, 10],
    ] as Point[])
      t.down(p(q), empty);
    expect(t.key('Enter').commit).toBeUndefined();
    expect(t.view().error).toBe('plan.errors.selfIntersecting');
    t.key('Backspace');
    expect(t.view().draft?.footprint).toHaveLength(3);
    expect(t.key('Escape').consumed).toBe(true);
    expect(t.view().draft).toBeNull();
    expect(t.key('Escape').consumed).toBe(false);
  });
});

describe('SelectTool', () => {
  it('drags a vertex with snapping and commits the footprint', () => {
    const t = new SelectTool();
    expect(t.down(p([10.1, 6.1]), withHouse).consumed).toBe(true);
    t.move(p([12.2, 7.9]), withHouse);
    const r = t.up(p([12.2, 7.9]), withHouse);
    expect(r.commit?.footprint[2]).toEqual([12, 8]);
    expect(r.commit?.changes).toEqual([{ kind: 'none' }]);
  });

  it('inserts a vertex from the middle of an edge', () => {
    const t = new SelectTool();
    t.down(p([5, 0]), withHouse);
    t.move(p([5, -2]), withHouse);
    const r = t.up(p([5, -2]), withHouse);
    expect(r.commit?.footprint).toHaveLength(5);
    expect(r.commit?.footprint[1]).toEqual([5, -2]);
    expect(r.commit?.changes[0]).toEqual({ kind: 'insert', edge: 0 });
  });

  it('moves the whole building on the snap step', () => {
    const t = new SelectTool();
    t.down(p([3, 3]), withHouse);
    t.move(p([6.13, 1.08]), withHouse);
    const r = t.up(p([6.13, 1.08]), withHouse);
    expect(r.commit?.delta).toEqual([3, -2]);
    expect(r.commit?.footprint[0]).toEqual([3, -2]);
  });

  it('deletes the selected vertex, refuses a crossing, leaves other clicks to the plan', () => {
    const t = new SelectTool();
    t.down(p([10, 6]), withHouse);
    t.up(p([10, 6]), withHouse);
    expect(t.view().vertex).toBe(2);
    const r = t.key('Delete', withHouse);
    expect(r.commit?.footprint).toHaveLength(3);
    expect(r.commit?.changes[0]).toEqual({ kind: 'remove', vertex: 2 });

    const u = new SelectTool();
    u.down(p([0, 6]), withHouse);
    u.move(p([12, 3]), withHouse);
    expect(u.up(p([12, 3]), withHouse).commit).toBeUndefined();
    expect(u.view().error).toBe('plan.errors.selfIntersecting');

    expect(new SelectTool().down(p([30, 30]), withHouse).consumed).toBe(false);
  });
});
