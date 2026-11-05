import { create } from 'zustand';
import type { CommandError, ProjectSummary, StressPreset } from '../bindings';
import { api, BackendError, unwrap } from '../ipc';
import { decodeModuleBuffer, type ModuleData } from '../viewport3d/moduleBuffer';

const ALREADY_EXISTS = 'errors.store.alreadyExists';

/** Read-only projection of the project metadata owned by Rust. */
interface ProjectState {
  summary: ProjectSummary | null;
  /** Decoded PVS1 buffer (typed arrays; mutated in place by the viewport on sync). */
  modules: ModuleData | null;
  preset: StressPreset | null;
  loading: StressPreset | null;
  error: CommandError | null;
  /** `performance.now()` when `open_project` was called (open-to-first-frame metric). */
  openStartedAt: number | null;
  loadPreset: (preset: StressPreset) => Promise<void>;
  /** Applies counters returned by Rust after a command (undo/redo availability, counts). */
  patch: (
    patch: Partial<Pick<ProjectSummary, 'canUndo' | 'canRedo' | 'activeModuleCount'>>,
  ) => void;
}

function toCommandError(e: unknown): CommandError {
  if (e instanceof BackendError) return { key: e.key, params: e.params };
  return { key: 'errors.store.failure', params: { detail: String(e) } };
}

export const useProjectStore = create<ProjectState>()((set, get) => ({
  summary: null,
  modules: null,
  preset: null,
  loading: null,
  error: null,
  openStartedAt: null,

  loadPreset: async (preset) => {
    if (get().loading) return;
    set({ loading: preset, error: null });
    try {
      const path = await unwrap(api.stressProjectPath(preset));
      // Generate the file the first time; afterwards always measure a plain open.
      try {
        await unwrap(api.createStressProject(preset, path));
      } catch (e) {
        if (!(e instanceof BackendError && e.key === ALREADY_EXISTS)) throw e;
      }
      const openStartedAt = performance.now();
      const summary = await unwrap(api.openProject(path));
      const modules = decodeModuleBuffer(await unwrap(api.getModuleBuffer()));
      set({ summary, modules, preset, openStartedAt, loading: null });
    } catch (e) {
      set({ error: toCommandError(e), loading: null });
    }
  },

  patch: (patch) => {
    const { summary } = get();
    if (summary) set({ summary: { ...summary, ...patch } });
  },
}));
