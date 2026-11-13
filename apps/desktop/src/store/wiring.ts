import { create } from 'zustand';
import type { WiringReport_Serialize } from '../bindings';
import { api, unwrap } from '../ipc';

/** String ordinal of each module id (wiring order over the project), for the 3D colours. */
export type StringMap = ReadonlyMap<number, number>;

interface WiringState {
  report: WiringReport_Serialize | null;
  stringOf: StringMap;
  /** Fetches the checks of the open project again (after every model change). */
  refresh: () => Promise<void>;
  clear: () => void;
}

function stringMap(r: WiringReport_Serialize): StringMap {
  const map = new Map<number, number>();
  let n = 0;
  for (const inv of r.inverters) {
    for (const t of inv.trackers) {
      for (const s of t.strings) {
        for (const id of s.moduleIds) map.set(id, n);
        n += 1;
      }
    }
  }
  return map;
}

/** DC design checks of the open project (computed in Rust on demand). */
export const useWiringStore = create<WiringState>()((set) => ({
  report: null,
  stringOf: new Map(),
  refresh: async () => {
    const report = await unwrap(api.wiringReport());
    set({ report, stringOf: stringMap(report) });
  },
  clear: () => {
    set({ report: null, stringOf: new Map() });
  },
}));
