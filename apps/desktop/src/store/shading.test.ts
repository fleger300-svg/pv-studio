import { describe, expect, it } from 'vitest';
import { SHADING_STOPS, roofLossAt, shadingColor } from './shading';

describe('shadingColor', () => {
  it('gives the stop colours at the stops and clamps outside', () => {
    for (const s of SHADING_STOPS) expect(shadingColor(s.at)).toBe(s.color);
    expect(shadingColor(-1)).toBe(SHADING_STOPS[0]?.color);
    expect(shadingColor(Number.NaN)).toBe(SHADING_STOPS[0]?.color);
    expect(shadingColor(0.9)).toBe(SHADING_STOPS[SHADING_STOPS.length - 1]?.color);
  });

  it('interpolates between two stops', () => {
    // Half way between #e1bc29 (7 %) and #e8822a (12 %).
    expect(shadingColor(0.095)).toBe('#e59f2a');
  });
});

describe('roofLossAt', () => {
  const map = {
    faces: [
      {
        faceId: 'f',
        originUv: [1, 2] as [number, number],
        cellM: 0.5,
        cols: 2,
        rows: 2,
        losses: [100, -1, 250, 9999],
      },
    ],
  };
  it('reads the cell under a roof point', () => {
    expect(roofLossAt(map, 'f', 1.2, 2.2)).toBe(0.01);
    expect(roofLossAt(map, 'f', 1.2, 2.7)).toBe(0.025);
    expect(roofLossAt(map, 'f', 1.9, 2.9)).toBe(0.9999);
  });
  it('is null off the face, off the grid and for another face', () => {
    expect(roofLossAt(map, 'f', 1.7, 2.2)).toBeNull();
    expect(roofLossAt(map, 'f', 0.5, 2.2)).toBeNull();
    expect(roofLossAt(map, 'f', 1.2, 3.1)).toBeNull();
    expect(roofLossAt(map, 'g', 1.2, 2.2)).toBeNull();
    expect(roofLossAt(null, 'f', 1.2, 2.2)).toBeNull();
  });
});
