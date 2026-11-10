import { describe, expect, it } from 'vitest';
import { fit, gridStep, pan, toScreen, toWorld, visibleBounds, zoomAt } from './viewTransform';

const t = { scale: 10, x: 100, y: 200 };

describe('viewTransform', () => {
  it('maps world ↔ screen with north up', () => {
    expect(toScreen(t, [0, 0])).toEqual([100, 200]);
    expect(toScreen(t, [3, 2])).toEqual([130, 180]);
    expect(toWorld(t, [130, 180])).toEqual([3, 2]);
  });

  it('zooms around the cursor', () => {
    const at: [number, number] = [160, 140];
    const before = toWorld(t, at);
    const z = zoomAt(t, at, 2.5);
    expect(z.scale).toBe(25);
    const after = toWorld(z, at);
    expect(after[0]).toBeCloseTo(before[0], 12);
    expect(after[1]).toBeCloseTo(before[1], 12);
    expect(zoomAt(t, at, 1e9).scale).toBe(2000);
  });

  it('pans in pixels', () => {
    expect(pan(t, 5, -7)).toEqual({ scale: 10, x: 105, y: 193 });
  });

  it('fits bounds centred with a margin', () => {
    const f = fit({ minE: -10, minN: 0, maxE: 10, maxN: 5 }, 440, 300, 20);
    expect(f.scale).toBe(20);
    expect(toScreen(f, [0, 2.5])).toEqual([220, 150]);
    const v = visibleBounds(f, 440, 300);
    expect(v.minE).toBeLessThan(-10);
    expect(v.maxN).toBeGreaterThan(5);
  });

  it('picks a readable grid step', () => {
    expect(gridStep(300)).toBe(0.1);
    expect(gridStep(30)).toBe(1);
    expect(gridStep(3)).toBe(10);
    expect(gridStep(0.001)).toBe(500);
  });
});
