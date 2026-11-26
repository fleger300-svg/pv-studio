import { create } from 'zustand';
import type { RootState } from '@react-three/fiber';
import type { ColorMode } from '../viewport3d/PanelMeshes';
import type { Editor } from '../viewport3d/editor';

export type Tool = 'select' | 'box' | 'move';

/** The real sun of the 3-D view: day of the year (1–365) and Swiss legal time [min]. */
export interface SunTime {
  day: number;
  minute: number;
}

/** View-only state (never part of the model). */
interface ViewState {
  colorMode: ColorMode;
  tool: Tool;
  /** Mirrors `editor.selection.size` for the UI. */
  selectionCount: number;
  /** Imperative editing API of the mounted viewport (null when no project is shown). */
  editor: Editor | null;
  /** R3F state accessor of the mounted canvas (camera, controls, invalidate) for the benchmark. */
  three: (() => RootState) | null;
  /** Bumped to frame the whole project in the 3D view again. */
  frameRequest: number;
  /** The light is the real sun at this date and time; null: the fixed default light. */
  sun: SunTime | null;
  /** The terrain and neighbouring buildings of the surface model are drawn. */
  showSurroundings: boolean;
  setShowSurroundings: (show: boolean) => void;
  setSun: (sun: SunTime | null) => void;
  setColorMode: (mode: ColorMode) => void;
  setTool: (tool: Tool) => void;
  requestFrame: () => void;
}

export const useViewStore = create<ViewState>()((set) => ({
  colorMode: 'default',
  tool: 'select',
  frameRequest: 0,
  sun: null,
  showSurroundings: true,
  setShowSurroundings: (showSurroundings) => {
    set({ showSurroundings });
  },
  setSun: (sun) => {
    set({ sun });
  },
  requestFrame: () => {
    set((s) => ({ frameRequest: s.frameRequest + 1 }));
  },
  selectionCount: 0,
  editor: null,
  three: null,
  setColorMode: (colorMode) => {
    set({ colorMode });
  },
  setTool: (tool) => {
    set({ tool });
  },
}));
