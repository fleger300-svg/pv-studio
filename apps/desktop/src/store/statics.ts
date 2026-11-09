import { create } from 'zustand';
import type {
  CommandError,
  FieldReport_Serialize,
  ResultReady_Serialize,
  StaticsSnapshot_Serialize,
} from '../bindings';

/** Statics of one field, as the compute thread reports it. */
export type FieldReport = FieldReport_Serialize;

/** Reports by array id. */
export type FieldReports = Readonly<Record<string, FieldReport>>;

/**
 * Merges reports, keeping for each field the most recent one: a report computed for an older
 * command than the one already held is ignored (architecture: « Un résultat dont l'empreinte
 * est dépassée est ignoré »). Pure, for tests.
 */
export function mergeReports(fields: FieldReports, reports: readonly FieldReport[]): FieldReports {
  let next: Record<string, FieldReport> | null = null;
  for (const r of reports) {
    const held = (next ?? fields)[r.arrayId];
    if (held && held.seq > r.seq) continue;
    next ??= { ...fields };
    next[r.arrayId] = r;
  }
  return next ?? fields;
}

/** Applies a `result:ready` event. Pure, for tests. */
export function reduceReady(fields: FieldReports, e: ResultReady_Serialize): FieldReports {
  const removed = new Set(e.removed);
  const kept = e.reset
    ? {}
    : Object.fromEntries(Object.entries(fields).filter(([id]) => !removed.has(id)));
  return mergeReports(kept, e.fields);
}

interface StaticsState {
  fields: FieldReports;
  /** A pass is running on the compute thread. */
  busy: boolean;
  /** Command sequence number of the last pass state received. */
  seq: number;
  /** Last pass failure (cleared by the next results). */
  failure: CommandError | null;
  applyReady: (e: ResultReady_Serialize) => void;
  applyState: (busy: boolean, seq: number) => void;
  applyFailure: (error: CommandError) => void;
  applySnapshot: (s: StaticsSnapshot_Serialize) => void;
  clear: () => void;
}

/** Read-only projection of the statics computed in Rust (never computed here). */
export const useStaticsStore = create<StaticsState>()((set, get) => ({
  fields: {},
  busy: false,
  seq: 0,
  failure: null,

  applyReady: (e) => {
    set({ fields: reduceReady(get().fields, e), failure: null });
  },
  applyState: (busy, seq) => {
    // Events of one thread arrive in order; a snapshot read meanwhile may be older.
    if (seq >= get().seq) set({ busy, seq });
  },
  applyFailure: (error) => {
    set({ failure: error });
  },
  applySnapshot: (s) => {
    const { seq } = get();
    set({
      fields: mergeReports(get().fields, s.fields),
      ...(s.seq >= seq ? { busy: s.busy, seq: s.seq } : {}),
    });
  },
  clear: () => {
    set({ fields: {}, busy: false, seq: 0, failure: null });
  },
}));
