import { create } from 'zustand';
import type { ToolId } from '../editor2d/tools';
import type { ViewTransform } from '../editor2d/viewTransform';

/** View state of the 2D plan (never model data). */
interface PlanState {
  /** `null` until the plan has been fitted to the project once. */
  transform: ViewTransform | null;
  /** Bumped to ask the plan to fit the project in view again. */
  fitRequest: number;
  /** Width share of the plan in the split view (0.2 to 0.8). */
  ratio: number;
  tool: ToolId;
  /**
   * Keys for the plan (set by the mounted plan view): `true` when the plan used the key, so
   * the default shortcut must not run.
   */
  keyHandler: ((key: string) => boolean) | null;
  setTool: (tool: ToolId) => void;
  setKeyHandler: (handler: ((key: string) => boolean) | null) => void;
  setTransform: (t: ViewTransform | null) => void;
  requestFit: () => void;
  setRatio: (ratio: number) => void;
}

const RATIO_KEY = 'pv.planRatio';

function initialRatio(): number {
  try {
    const v = Number(localStorage.getItem(RATIO_KEY));
    return v >= 0.2 && v <= 0.8 ? v : 0.5;
  } catch {
    return 0.5;
  }
}

export const usePlanStore = create<PlanState>()((set) => ({
  transform: null,
  fitRequest: 0,
  ratio: initialRatio(),
  tool: 'select',
  keyHandler: null,
  setTool: (tool) => {
    set({ tool });
  },
  setKeyHandler: (keyHandler) => {
    set({ keyHandler });
  },
  setTransform: (transform) => {
    set({ transform });
  },
  requestFit: () => {
    set((s) => ({ fitRequest: s.fitRequest + 1 }));
  },
  setRatio: (ratio) => {
    const r = Math.min(0.8, Math.max(0.2, ratio));
    try {
      localStorage.setItem(RATIO_KEY, String(r));
    } catch {
      // Per-viewer convenience only.
    }
    set({ ratio: r });
  },
}));
