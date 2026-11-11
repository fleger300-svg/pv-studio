import { create } from 'zustand';

/** Settings view state (UI only: the API key itself never reaches the front end). */
interface SettingsState {
  open: boolean;
  setOpen: (open: boolean) => void;
}

export const useSettingsStore = create<SettingsState>()((set) => ({
  open: false,
  setOpen: (open) => {
    set({ open });
  },
}));
