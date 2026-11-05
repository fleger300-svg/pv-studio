import { create } from 'zustand';
import type { Theme } from '@pv/ui';

const THEME_KEY = 'pv.theme';

function initialTheme(): Theme {
  const stored = localStorage.getItem(THEME_KEY);
  if (stored === 'light' || stored === 'dark') return stored;
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

/** UI-only state (never model data — the model lives in Rust). */
interface UiState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

export const useUiStore = create<UiState>()((set) => {
  const theme = initialTheme();
  applyTheme(theme);
  return {
    theme,
    setTheme: (next) => {
      applyTheme(next);
      localStorage.setItem(THEME_KEY, next);
      set({ theme: next });
    },
  };
});
