import { describe, expect, it } from 'vitest';
import { SHADING_STOPS, shadingColor } from './shading';

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
