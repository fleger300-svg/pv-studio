import { describe, expect, it } from 'vitest';
import { SHADING_STOPS } from '../store/shading';
import { gridPixels } from './RoofShading';

describe('gridPixels', () => {
  it('colours the cells on the face and leaves the others transparent', () => {
    const px = gridPixels([0, -1, 2000]);
    expect(Array.from(px.subarray(0, 4))).toEqual([0x2e, 0x9e, 0x4f, 255]);
    expect(Array.from(px.subarray(4, 8))).toEqual([0, 0, 0, 0]);
    const red = SHADING_STOPS[SHADING_STOPS.length - 1]?.color ?? '';
    expect(px[8]).toBe(Number.parseInt(red.slice(1, 3), 16));
    expect(px[11]).toBe(255);
  });
});
