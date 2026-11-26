import { create } from 'zustand';
import type { CommandError, RoofShadingMap, ShadingMap } from '../bindings';
import { toCommandError } from '../commands/runCommand';
import { api, unwrap } from '../ipc';

/** Shading of one module over the year (shares of its irradiation). */
export interface ModuleShade {
  horizon: number;
  near: number;
}

/** Colour stops of the shading scale: total loss share → colour (green → yellow → red). */
export const SHADING_STOPS: readonly { at: number; color: string }[] = [
  { at: 0, color: '#2e9e4f' },
  { at: 0.03, color: '#9cc93b' },
  { at: 0.07, color: '#e1bc29' },
  { at: 0.12, color: '#e8822a' },
  { at: 0.2, color: '#d1495b' },
];

function hexToRgb(hex: string): [number, number, number] {
  const n = Number.parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** Colour (`#rrggbb`) of a total loss share, interpolated between the stops. Pure. */
export function shadingColor(loss: number): string {
  const stops = SHADING_STOPS;
  const first = stops[0];
  const last = stops[stops.length - 1];
  if (!first || !last) return '#000000';
  if (!(loss > first.at)) return first.color;
  if (loss >= last.at) return last.color;
  let i = 1;
  while (i < stops.length - 1 && (stops[i]?.at ?? 0) < loss) i += 1;
  const a = stops[i - 1] ?? first;
  const b = stops[i] ?? last;
  const t = (loss - a.at) / (b.at - a.at);
  const [ar, ag, ab] = hexToRgb(a.color);
  const [br, bg, bb] = hexToRgb(b.color);
  const mix = (x: number, y: number) =>
    Math.round(x + (y - x) * t)
      .toString(16)
      .padStart(2, '0');
  return `#${mix(ar, br)}${mix(ag, bg)}${mix(ab, bb)}`;
}

/** What the pointer is over in the 3-D view (shading mode): a module and/or a roof point,
 *  with the pointer position in the viewport [px]. */
export interface ShadingHover {
  x: number;
  y: number;
  /** Module id, or null over the bare roof. */
  id: number | null;
  /** Roof face and point under the pointer. */
  face: string;
  u: number;
  v: number;
}

/** Loss share of the roof map at `(u, v)` of a face; null off the map. Pure. */
export function roofLossAt(
  map: RoofShadingMap | null,
  face: string,
  u: number,
  v: number,
): number | null {
  const g = map?.faces.find((f) => f.faceId === face);
  if (!g) return null;
  const c = Math.floor((u - g.originUv[0]) / g.cellM);
  const r = Math.floor((v - g.originUv[1]) / g.cellM);
  if (c < 0 || r < 0 || c >= g.cols || r >= g.rows) return null;
  const x = g.losses[r * g.cols + c] ?? -1;
  return x < 0 ? null : x / 10000;
}

interface ShadingState {
  /** Shading by module id; empty until computed. */
  byModule: ReadonlyMap<number, ModuleShade>;
  /** Irradiation-weighted means over the modules. */
  mean: ModuleShade | null;
  busy: boolean;
  error: CommandError | null;
  /** Shading map of the roof faces; null until computed. */
  roof: RoofShadingMap | null;
  roofBusy: boolean;
  /** The roof map is drawn (shading mode). */
  showRoof: boolean;
  hover: ShadingHover | null;
  /** Computes the shading of the open project again (latest request wins). */
  refresh: () => Promise<void>;
  setHover: (hover: ShadingHover | null) => void;
  setShowRoof: (show: boolean) => void;
  clear: () => void;
}

let request = 0;
let roofRequest = 0;

async function refreshRoof(set: (s: Partial<ShadingState>) => void): Promise<void> {
  const mine = ++roofRequest;
  set({ roofBusy: true });
  try {
    const roof = await unwrap(api.roofShading());
    if (mine === roofRequest) set({ roof, roofBusy: false });
  } catch {
    // The module shading reports the error (same causes).
    if (mine === roofRequest) set({ roof: null, roofBusy: false });
  }
}

/** Shading of each module and of the roofs of the open project (computed in Rust on demand). */
export const useShadingStore = create<ShadingState>()((set) => ({
  byModule: new Map(),
  mean: null,
  busy: false,
  error: null,
  roof: null,
  roofBusy: false,
  showRoof: true,
  hover: null,
  setHover: (hover) => {
    set({ hover });
  },
  setShowRoof: (showRoof) => {
    set({ showRoof });
  },
  refresh: async () => {
    const mine = ++request;
    set({ busy: true });
    const roof = refreshRoof(set);
    try {
      const map: ShadingMap = await unwrap(api.moduleShading());
      if (mine !== request) return;
      set({
        byModule: new Map(
          map.modules.map((m) => [m.moduleId, { horizon: m.horizon, near: m.near }]),
        ),
        mean: { horizon: map.horizon, near: map.near },
        busy: false,
        error: null,
      });
    } catch (e) {
      if (mine !== request) return;
      set({ byModule: new Map(), mean: null, busy: false, error: toCommandError(e) });
    }
    await roof;
  },
  clear: () => {
    request += 1;
    roofRequest += 1;
    set({
      byModule: new Map(),
      mean: null,
      busy: false,
      error: null,
      roof: null,
      roofBusy: false,
      hover: null,
    });
  },
}));
