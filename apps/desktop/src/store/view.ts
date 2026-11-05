import { create } from 'zustand';
import type { ColorMode } from '../viewport3d/PanelMeshes';

/** View-only state (never part of the model). */
interface ViewState {
  colorMode: ColorMode;
  setColorMode: (mode: ColorMode) => void;
}

export const useViewStore = create<ViewState>()((set) => ({
  colorMode: 'default',
  setColorMode: (colorMode) => {
    set({ colorMode });
  },
}));
