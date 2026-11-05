import { describe, expect, it } from 'vitest';
import { percentile, rollingFps, summarizeFrames } from './stats';

describe('frame statistics', () => {
  it('percentile uses nearest rank', () => {
    const values = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(values, 95)).toBe(95);
    expect(percentile(values, 50)).toBe(50);
    expect(percentile([], 95)).toBe(0);
  });

  it('summarises a steady 60 Hz run with one hitch', () => {
    const stamps: number[] = [];
    let t = 0;
    for (let i = 0; i < 180; i++) {
      stamps.push(t);
      t += i === 90 ? 50 : 1000 / 60;
    }
    const s = summarizeFrames(stamps);
    expect(s.frames).toBe(180);
    expect(s.avgFps).toBeGreaterThan(57);
    expect(s.avgFps).toBeLessThan(60);
    expect(s.maxFrameMs).toBeCloseTo(50);
    expect(s.p95FrameMs).toBeCloseTo(1000 / 60);
    expect(s.minFps).toBeGreaterThanOrEqual(57);
    expect(s.medianFrameMs).toBeCloseTo(1000 / 60);
  });

  it('rolling FPS counts frames in the last second', () => {
    const stamps = Array.from({ length: 120 }, (_, i) => i * 10);
    expect(rollingFps(stamps, 1190)).toBe(100);
  });
});
