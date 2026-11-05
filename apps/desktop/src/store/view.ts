import { create } from 'zustand';
import type { RootState } from '@react-three/fiber';
import type { ColorMode } from '../viewport3d/PanelMeshes';
import type { Editor } from '../viewport3d/editor';

export type Tool = 'select' | 'box' | 'move';

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
  setColorMode: (mode: ColorMode) => void;
  setTool: (tool: Tool) => void;
}

export const useViewStore = create<ViewState>()((set) => ({
  colorMode: 'default',
  tool: 'select',
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
