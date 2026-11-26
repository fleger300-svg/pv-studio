import { create } from 'zustand';
import type { CommandError, ShadingMap } from '../bindings';
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

interface ShadingState {
  /** Shading by module id; empty until computed. */
  byModule: ReadonlyMap<number, ModuleShade>;
  /** Irradiation-weighted means over the modules. */
  mean: ModuleShade | null;
  busy: boolean;
  error: CommandError | null;
  /** Module under the pointer in the 3-D view (shading mode), with the pointer position in
   *  the viewport [px]. */
  hover: { id: number; x: number; y: number } | null;
  /** Computes the shading of the open project again (latest request wins). */
  refresh: () => Promise<void>;
  setHover: (hover: { id: number; x: number; y: number } | null) => void;
  clear: () => void;
}

let request = 0;

/** Shading of each module of the open project (computed in Rust on demand). */
export const useShadingStore = create<ShadingState>()((set) => ({
  byModule: new Map(),
  mean: null,
  busy: false,
  error: null,
  hover: null,
  setHover: (hover) => {
    set({ hover });
  },
  refresh: async () => {
    const mine = ++request;
    set({ busy: true });
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
  },
  clear: () => {
    request += 1;
    set({ byModule: new Map(), mean: null, busy: false, error: null, hover: null });
  },
}));
