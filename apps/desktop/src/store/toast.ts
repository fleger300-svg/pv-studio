import { create } from 'zustand';
import type { CommandError } from '../bindings';

export interface Toast {
  id: number;
  /** Translatable message (i18n key + ICU params). */
  message: CommandError;
}

interface ToastState {
  toasts: Toast[];
  push: (message: CommandError) => void;
  dismiss: (id: number) => void;
}

const LIFETIME_MS = 5000;
let nextId = 1;

export const useToastStore = create<ToastState>()((set, get) => ({
  toasts: [],
  push: (message) => {
    const id = nextId++;
    set({ toasts: [...get().toasts.slice(-3), { id, message }] });
    setTimeout(() => {
      get().dismiss(id);
    }, LIFETIME_MS);
  },
  dismiss: (id) => {
    set({ toasts: get().toasts.filter((t) => t.id !== id) });
  },
}));
