/**
 * Sun position for the 3-D view: the NOAA formulas of `pv_solar::sun::position` (Rust), so
 * that the shadows drawn match the simulation's sun, and Swiss legal time (CET, CEST from the
 * last Sunday of March to the last Sunday of October, at 01:00 UTC).
 */

/** Year the dates are shown in (non-leap, as the weather year). */
export const SUN_YEAR = 2026;

export interface SunPosition {
  /** From north, clockwise [°]. */
  azimuthDeg: number;
  /** Above the horizon [°] (negative at night). */
  elevationDeg: number;
}

const rad = Math.PI / 180;

/** Sun position at `hourUtc` (fractional) of day `day` (1–365) of a non-leap year. Pure. */
export function sunPosition(latDeg: number, lonDeg: number, day: number, hourUtc: number) {
  const g = ((2 * Math.PI) / 365) * (day - 1 + (hourUtc - 12) / 24);
  const eqtime =
    229.18 *
    (0.000075 +
      0.001868 * Math.cos(g) -
      0.032077 * Math.sin(g) -
      0.014615 * Math.cos(2 * g) -
      0.040849 * Math.sin(2 * g));
  const decl =
    0.006918 -
    0.399912 * Math.cos(g) +
    0.070257 * Math.sin(g) -
    0.006758 * Math.cos(2 * g) +
    0.000907 * Math.sin(2 * g) -
    0.002697 * Math.cos(3 * g) +
    0.00148 * Math.sin(3 * g);
  const trueSolarMin = hourUtc * 60 + eqtime + 4 * lonDeg;
  const ha = (trueSolarMin / 4 - 180) * rad;
  const lat = latDeg * rad;
  const cosZ = Math.min(
    1,
    Math.max(-1, Math.sin(lat) * Math.sin(decl) + Math.cos(lat) * Math.cos(decl) * Math.cos(ha)),
  );
  const zenith = Math.acos(cosZ);
  const sinZ = Math.max(Math.sin(zenith), 1e-9);
  const cosAz = Math.min(
    1,
    Math.max(-1, (Math.sin(lat) * cosZ - Math.sin(decl)) / (Math.cos(lat) * sinZ)),
  );
  const az = Math.acos(cosAz) / rad;
  const azimuth = ha > 0 ? 180 + az : 180 - az;
  return {
    azimuthDeg: ((azimuth % 360) + 360) % 360,
    elevationDeg: 90 - zenith / rad,
  } satisfies SunPosition;
}

/** Unit vector towards the sun (world ENU, z up). Pure. */
export function sunDirection(p: SunPosition): [number, number, number] {
  const az = p.azimuthDeg * rad;
  const el = p.elevationDeg * rad;
  return [Math.sin(az) * Math.cos(el), Math.cos(az) * Math.cos(el), Math.sin(el)];
}

/** Day of year (1–365) of the last Sunday of `month` (0-based) in `SUN_YEAR`. */
function lastSunday(month: number): number {
  const last = new Date(Date.UTC(SUN_YEAR, month + 1, 0));
  last.setUTCDate(last.getUTCDate() - last.getUTCDay());
  return dayOfYear(last);
}

/** Day of year (1–365) of a date (its UTC calendar day). */
export function dayOfYear(d: Date): number {
  const start = Date.UTC(d.getUTCFullYear(), 0, 1);
  const day =
    Math.floor((Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()) - start) / 86400000) +
    1;
  return Math.min(365, day);
}

/** Offset of Swiss legal time from UTC [h] at local `minute` of `day`. Pure. */
export function swissUtcOffset(day: number, minute: number): number {
  const start = lastSunday(2);
  const end = lastSunday(9);
  // Changes at 02:00 CET (spring) and 03:00 CEST (autumn), both 01:00 UTC.
  const summer =
    (day > start || (day === start && minute >= 120)) &&
    (day < end || (day === end && minute < 180));
  return summer ? 2 : 1;
}

/** Sun position at Swiss legal time `minute` (0–1439) of `day`. Pure. */
export function sunAtLocal(latDeg: number, lonDeg: number, day: number, minute: number) {
  return sunPosition(latDeg, lonDeg, day, minute / 60 - swissUtcOffset(day, minute));
}

/** Sunrise and sunset in Swiss legal minutes of `day` (to 5 min), or null (polar). Pure. */
export function daylight(
  latDeg: number,
  lonDeg: number,
  day: number,
): { rise: number; set: number } | null {
  let rise: number | null = null;
  let set: number | null = null;
  for (let m = 0; m < 1440; m += 5) {
    const up = sunAtLocal(latDeg, lonDeg, day, m).elevationDeg > 0;
    if (up && rise === null) rise = m;
    if (up) set = m;
  }
  return rise === null || set === null ? null : { rise, set };
}

/** The date of `day` in `SUN_YEAR` (UTC noon, for formatting). */
export function dateOfDay(day: number): Date {
  return new Date(Date.UTC(SUN_YEAR, 0, day, 12));
}
