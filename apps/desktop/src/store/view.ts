import { create } from 'zustand';
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
  setColorMode: (mode: ColorMode) => void;
  setTool: (tool: Tool) => void;
}

export const useViewStore = create<ViewState>()((set) => ({
  colorMode: 'default',
  tool: 'select',
  selectionCount: 0,
  editor: null,
  setColorMode: (colorMode) => {
    set({ colorMode });
  },
  setTool: (tool) => {
    set({ tool });
  },
}));
