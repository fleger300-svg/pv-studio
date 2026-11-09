import { create } from 'zustand';
import type { CommandError, ProjectSummary, StressPreset } from '../bindings';
import { api, BackendError, unwrap } from '../ipc';
import { decodeModuleBuffer, type ModuleData } from '../viewport3d/moduleBuffer';

/** Counters that change with every command (kept apart so `summary` stays immutable). */
export interface ProjectCounters {
  canUndo: boolean;
  canRedo: boolean;
  activeModuleCount: number;
}

/** Read-only projection of the project owned by Rust. */
interface ProjectState {
  /**
   * Static description of the open project. The viewport builds its GPU meshes from it:
   * module edits never replace it; only commands that change entities (buildings, roof
   * faces, arrays, added or removed modules) do, through `reload`.
   */
  summary: ProjectSummary | null;
  /** Decoded PVS1 buffer (typed arrays; updated in place by the viewport on sync). */
  modules: ModuleData | null;
  counters: ProjectCounters;
  preset: StressPreset | null;
  loading: StressPreset | null;
  error: CommandError | null;
  /** `performance.now()` when `open_project` was called (open-to-first-frame metric). */
  openStartedAt: number | null;
  loadPreset: (preset: StressPreset) => Promise<void>;
  /** Applies the counters returned by Rust after a command. */
  setCounters: (counters: ProjectCounters) => void;
  /**
   * Re-reads the summary and every module after a command that changed entities or
   * renumbered modules: the viewport rebuilds its meshes from the new objects.
   */
  reload: () => Promise<void>;
}

function toCommandError(e: unknown): CommandError {
  if (e instanceof BackendError) return { key: e.key, params: e.params };
  return { key: 'errors.store.failure', params: { detail: String(e) } };
}

export const useProjectStore = create<ProjectState>()((set, get) => ({
  summary: null,
  modules: null,
  counters: { canUndo: false, canRedo: false, activeModuleCount: 0 },
  preset: null,
  loading: null,
  error: null,
  openStartedAt: null,

  loadPreset: async (preset) => {
    if (get().loading) return;
    set({ loading: preset, error: null });
    try {
      const path = await unwrap(api.stressProjectPath(preset));
      // Regenerate the synthetic file (same state every time), then measure a plain open.
      await unwrap(api.createStressProject(preset, path));
      const openStartedAt = performance.now();
      const summary = await unwrap(api.openProject(path));
      const modules = decodeModuleBuffer(await unwrap(api.getModuleBuffer()));
      const { canUndo, canRedo, activeModuleCount } = summary;
      set({
        summary,
        modules,
        counters: { canUndo, canRedo, activeModuleCount },
        preset,
        openStartedAt,
        loading: null,
      });
    } catch (e) {
      set({ error: toCommandError(e), loading: null });
    }
  },

  setCounters: (counters) => {
    set({ counters });
  },

  reload: async () => {
    if (!get().summary) return;
    try {
      const summary = await unwrap(api.getProjectSummary());
      const modules = decodeModuleBuffer(await unwrap(api.getModuleBuffer()));
      const { canUndo, canRedo, activeModuleCount } = summary;
      set({ summary, modules, counters: { canUndo, canRedo, activeModuleCount } });
    } catch (e) {
      set({ error: toCommandError(e) });
    }
  },
}));
