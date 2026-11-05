import { describe, expect, it } from 'vitest';
import { hitRoofs, planeUV } from './roofHit';
import { flatRoofFixture } from './testSummary';

describe('roof-plane hit', () => {
  const { summary } = flatRoofFixture();

  it('converts a downward ray to roof (u, v)', () => {
    const hit = hitRoofs(summary.roofFaces, [2.5, 4, 20], [0, 0, -1]);
    expect(hit?.faceIndex).toBe(0);
    expect(hit?.u).toBeCloseTo(2.5);
    expect(hit?.v).toBeCloseTo(4);
    expect(hit?.t).toBeCloseTo(17);
  });

  it('misses outside the face rectangle and behind the camera', () => {
    expect(hitRoofs(summary.roofFaces, [12, 4, 20], [0, 0, -1])).toBeNull();
    expect(hitRoofs(summary.roofFaces, [2, 4, 20], [0, 0, 1])).toBeNull();
  });

  it('keeps intersecting the plane beyond the rectangle (for drags)', () => {
    const face = summary.roofFaces[0];
    expect(face && planeUV(face, [12, 4, 20], [0, 0, -1])?.u).toBeCloseTo(12);
  });
});
