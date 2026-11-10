import { describe, expect, it } from 'vitest';
import {
  deleteVertex,
  edgeLength,
  ensureCcw,
  insertVertex,
  isConvex,
  isSimple,
  nearestEdge,
  nearestVertex,
  rectangle,
  setEdgeLength,
  signedArea,
  syncEdges,
} from './polygonEdit';
import type { Point } from './viewTransform';

const L: Point[] = [
  [0, 0],
  [20, 0],
  [20, 10],
  [10, 10],
  [10, 20],
  [0, 20],
];

describe('polygonEdit', () => {
  it('builds counter-clockwise rectangles from any corners', () => {
    const r = rectangle([5, 4], [-1, 1]);
    expect(r).toEqual([
      [-1, 1],
      [5, 1],
      [5, 4],
      [-1, 4],
    ]);
    expect(signedArea(r)).toBe(18);
  });

  it('reverses clockwise polygons and their edge data together', () => {
    const cw = [...L].reverse();
    const edges = cw.map((_, i) => `e${i}`);
    const { poly, change } = ensureCcw(cw);
    expect(change.kind).toBe('reverse');
    expect(signedArea(poly)).toBeGreaterThan(0);
    const synced = syncEdges(edges, change);
    // Each edge keeps its data: new edge k is the same segment as old edge synced[k].
    poly.forEach((a, k) => {
      const b = poly[(k + 1) % poly.length];
      const old = Number(synced[k]?.slice(1));
      const oa = cw[old];
      const ob = cw[(old + 1) % cw.length];
      expect([a, b]).toEqual([ob, oa]);
    });
  });

  it('keeps edge data in step with inserted and removed vertices', () => {
    const edges = ['s', 'e', 'n1', 'w1', 'n2', 'w2'];
    const ins = insertVertex(L, 0, [8, 0]);
    expect(ins).toHaveLength(7);
    expect(syncEdges(edges, { kind: 'insert', edge: 0 })).toEqual([
      's',
      's',
      'e',
      'n1',
      'w1',
      'n2',
      'w2',
    ]);
    const del = deleteVertex(L, 3);
    expect(del).toHaveLength(5);
    // Vertex 3 joined edges 2 (n1) and 3 (w1): the merged edge keeps n1.
    expect(syncEdges(edges, { kind: 'remove', vertex: 3 })).toEqual(['s', 'e', 'n1', 'n2', 'w2']);
    expect(syncEdges(edges, { kind: 'remove', vertex: 0 })).toEqual(['e', 'n1', 'w1', 'n2', 'w2']);
    expect(deleteVertex(rectangle([0, 0], [1, 1]), 0)).toHaveLength(3);
    expect(
      deleteVertex(
        [
          [0, 0],
          [1, 0],
          [0, 1],
        ],
        0,
      ),
    ).toBeNull();
  });

  it('stretches along an edge to set its length', () => {
    const r = setEdgeLength(rectangle([0, 0], [10, 6]), 0, 12);
    expect(r).toEqual([
      [0, 0],
      [12, 0],
      [12, 6],
      [0, 6],
    ]);
    // The L keeps its right angles: everything right of x = 20 moves.
    const l = setEdgeLength(L, 0, 25);
    expect(edgeLength(l, 0)).toBeCloseTo(25, 12);
    expect(l[2]).toEqual([25, 10]);
    expect(l[3]).toEqual([10, 10]);
  });

  it('finds vertices and edges near a point', () => {
    expect(nearestVertex(L, [19.6, 0.2], 0.5)).toBe(1);
    expect(nearestVertex(L, [15, 5], 0.5)).toBe(-1);
    expect(nearestEdge(L, [15, 9.8], 0.5)).toEqual({ edge: 2, point: [15, 10] });
  });

  it('tells convex from concave', () => {
    expect(isConvex(rectangle([0, 0], [3, 2]))).toBe(true);
    expect(isConvex(L)).toBe(false);
  });

  it('detects self-intersections', () => {
    expect(isSimple(L)).toBe(true);
    expect(
      isSimple([
        [0, 0],
        [4, 4],
        [4, 0],
        [0, 4],
      ]),
    ).toBe(false);
  });
});
