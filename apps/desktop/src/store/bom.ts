import { create } from 'zustand';
import type { BomReport_Serialize } from '../bindings';

/** The project's bill of materials, as the compute thread reports it. */
export type BomReport = BomReport_Serialize;

/**
 * Next report to hold: a report computed for an older command than the one held is ignored;
 * `reset` (new project) drops the one held. Pure, for tests.
 */
export function nextBom(
  held: BomReport | null,
  incoming: BomReport | null,
  reset: boolean,
): BomReport | null {
  const base = reset ? null : held;
  if (!incoming) return base;
  if (base && base.seq > incoming.seq) return base;
  return incoming;
}

interface BomState {
  report: BomReport | null;
  /** From a `result:ready` event (`bom` is `null` when it did not change). */
  apply: (incoming: BomReport | null, reset: boolean) => void;
  clear: () => void;
}

/** Read-only projection of the bill of materials computed in Rust. */
export const useBomStore = create<BomState>()((set, get) => ({
  report: null,
  apply: (incoming, reset) => {
    const next = nextBom(get().report, incoming, reset);
    if (next !== get().report) set({ report: next });
  },
  clear: () => {
    set({ report: null });
  },
}));
