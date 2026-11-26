import { describe, expect, it } from 'vitest';
import { daylight, sunAtLocal, sunDirection, sunPosition, swissUtcOffset } from './sun';

describe('sunPosition', () => {
  it('gives the values of the Rust simulation (pv_solar::sun::position)', () => {
    // Bern; references printed by the Rust function.
    const cases: [number, number, number, number][] = [
      [172, 11, 24.3327, 162.2203],
      [355, 8.5, 81.0004, 139.3744],
      [80, 15.25, 66.4996, 242.2389],
    ];
    for (const [day, hour, zenith, azimuth] of cases) {
      const p = sunPosition(46.948, 7.447, day, hour);
      expect(90 - p.elevationDeg).toBeCloseTo(zenith, 3);
      expect(p.azimuthDeg).toBeCloseTo(azimuth, 3);
    }
  });

  it('points the light towards the sun', () => {
    const [x, y, z] = sunDirection({ azimuthDeg: 180, elevationDeg: 30 });
    expect(x).toBeCloseTo(0);
    expect(y).toBeCloseTo(-Math.cos(Math.PI / 6));
    expect(z).toBeCloseTo(0.5);
  });
});

describe('Swiss legal time', () => {
  it('switches to summer time on the last Sundays of March and October 2026', () => {
    // 29 March 2026 = day 88; 25 October 2026 = day 298.
    expect(swissUtcOffset(87, 720)).toBe(1);
    expect(swissUtcOffset(88, 60)).toBe(1);
    expect(swissUtcOffset(88, 180)).toBe(2);
    expect(swissUtcOffset(200, 720)).toBe(2);
    expect(swissUtcOffset(298, 120)).toBe(2);
    expect(swissUtcOffset(298, 240)).toBe(1);
    expect(swissUtcOffset(355, 720)).toBe(1);
  });

  it('puts the summer noon sun high in the south and nights below the horizon', () => {
    // Bern, 21 June, 13:30 CEST ≈ solar noon.
    const noon = sunAtLocal(46.948, 7.447, 172, 13 * 60 + 30);
    expect(noon.elevationDeg).toBeGreaterThan(65);
    expect(Math.abs(noon.azimuthDeg - 180)).toBeLessThan(5);
    expect(sunAtLocal(46.948, 7.447, 172, 60).elevationDeg).toBeLessThan(0);
    const june = daylight(46.948, 7.447, 172);
    const december = daylight(46.948, 7.447, 355);
    // ≈ 05:35–21:30 and 08:10–16:45 legal time.
    expect(june?.rise).toBeGreaterThan(5 * 60);
    expect(june?.rise).toBeLessThan(6 * 60);
    expect(june?.set).toBeGreaterThan(21 * 60);
    expect(december?.rise).toBeGreaterThan(8 * 60);
    expect(december?.set).toBeLessThan(17 * 60);
  });
});
