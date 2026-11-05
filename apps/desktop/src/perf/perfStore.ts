import { create } from 'zustand';
import type { CommandTiming } from '../viewport3d/editor';

export type BenchStep = 'open' | 'orbit' | 'drag' | 'move' | 'undo' | 'redo' | 'report';

export type BenchStatus =
  | { state: 'idle' }
  | { state: 'running'; step: BenchStep }
  | { state: 'done'; path: string }
  | { state: 'failed'; key: string; params: Record<string, string | number> };

interface PerfState {
  hudVisible: boolean;
  /** Project open → first rendered frame, ms (null until measured). */
  openMs: number | null;
  lastCommand: CommandTiming | null;
  bench: BenchStatus;
  toggleHud: () => void;
  setOpenMs: (ms: number) => void;
  resetOpenMs: () => void;
  recordCommand: (timing: CommandTiming) => void;
  setBench: (status: BenchStatus) => void;
}

export const usePerfStore = create<PerfState>()((set) => ({
  hudVisible: false,
  openMs: null,
  lastCommand: null,
  bench: { state: 'idle' },
  toggleHud: () => {
    set((s) => ({ hudVisible: !s.hudVisible }));
  },
  setOpenMs: (openMs) => {
    set({ openMs });
  },
  resetOpenMs: () => {
    set({ openMs: null });
  },
  recordCommand: (lastCommand) => {
    set({ lastCommand });
  },
  setBench: (bench) => {
    set({ bench });
  },
}));
