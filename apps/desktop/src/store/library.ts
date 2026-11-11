import { create } from 'zustand';
import type { ComponentId, UpdateProposal } from '../bindings';
import { api, unwrap } from '../ipc';
import { toCommandError } from '../commands/runCommand';
import { useToastStore } from './toast';

/**
 * Component library view state, and the update proposals of the open project. The library
 * itself lives in Rust (`library.sqlite`); this is only what the UI shows.
 */
interface LibraryState {
  /** The library view covers the main area. */
  open: boolean;
  setOpen: (open: boolean) => void;
  /** Selected component in the library view. */
  selected: ComponentId | null;
  select: (id: ComponentId | null) => void;
  /** Bumped after every library change (lists reload). */
  version: number;
  changed: () => void;
  /** Newer library versions of the project's frozen components. */
  proposals: UpdateProposal[];
  refreshProposals: () => Promise<void>;
}

export const useLibraryStore = create<LibraryState>()((set, get) => ({
  open: false,
  setOpen: (open) => {
    set({ open });
  },
  selected: null,
  select: (selected) => {
    set({ selected });
  },
  version: 0,
  changed: () => {
    set({ version: get().version + 1 });
    void get().refreshProposals();
  },
  proposals: [],
  refreshProposals: async () => {
    try {
      set({ proposals: await unwrap(api.componentUpdates()) });
    } catch (e) {
      const error = toCommandError(e);
      // No open project: nothing to propose.
      if (error.key !== 'errors.project.notOpen') useToastStore.getState().push(error);
      set({ proposals: [] });
    }
  },
}));
