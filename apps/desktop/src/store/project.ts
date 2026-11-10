import { create } from 'zustand';
import type { CommandError, ProjectSummary, StressPreset } from '../bindings';
import { api, BackendError, unwrap } from '../ipc';
import { decodeModuleBuffer, type ModuleData } from '../viewport3d/moduleBuffer';
import { useSelectionStore } from './selection';
import { initialStep, useWorkflowStore } from './workflow';

/** Counters that change with every command (kept apart so `summary` stays immutable). */
export interface ProjectCounters {
  canUndo: boolean;
  canRedo: boolean;
  activeModuleCount: number;
}

/** What is being opened (buttons show it as busy). */
export type Loading = StressPreset | 'new' | 'open';

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
  loading: Loading | null;
  error: CommandError | null;
  /** `performance.now()` when `open_project` was called (open-to-first-frame metric). */
  openStartedAt: number | null;
  loadPreset: (preset: StressPreset) => Promise<void>;
  /** New empty project (native « Enregistrer sous »; `path` for automation). */
  newProject: (path?: string) => Promise<void>;
  /** Opens a project file (native « Ouvrir »; `path` for automation). */
  openProject: (path?: string) => Promise<void>;
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

async function loadModules(): Promise<ModuleData> {
  return decodeModuleBuffer(await unwrap(api.getModuleBuffer()));
}

export const useProjectStore = create<ProjectState>()((set, get) => {
  /** A project was opened: everything is replaced, the selection and the step reset. */
  const adopt = async (
    summary: ProjectSummary,
    preset: StressPreset | null,
    openStartedAt: number,
  ) => {
    const modules = await loadModules();
    const { canUndo, canRedo, activeModuleCount } = summary;
    useSelectionStore.getState().clear();
    useWorkflowStore.getState().setStep(initialStep(summary.arrays.length));
    set({
      summary,
      modules,
      counters: { canUndo, canRedo, activeModuleCount },
      preset,
      openStartedAt,
      loading: null,
    });
  };

  /** Runs an opening action; `null` from Rust means the user cancelled the dialog. */
  const opening = async (
    loading: Loading,
    open: () => Promise<{
      summary: ProjectSummary | null;
      preset: StressPreset | null;
      /** When the measured open started (defaults to the start of the action). */
      startedAt?: number;
    }>,
  ) => {
    if (get().loading) return;
    set({ loading, error: null });
    try {
      const actionStartedAt = performance.now();
      const { summary, preset, startedAt } = await open();
      if (summary) await adopt(summary, preset, startedAt ?? actionStartedAt);
      else set({ loading: null });
    } catch (e) {
      set({ error: toCommandError(e), loading: null });
    }
  };

  return {
    summary: null,
    modules: null,
    counters: { canUndo: false, canRedo: false, activeModuleCount: 0 },
    preset: null,
    loading: null,
    error: null,
    openStartedAt: null,

    loadPreset: (preset) =>
      opening(preset, async () => {
        const path = await unwrap(api.stressProjectPath(preset));
        // Regenerate the synthetic file (same state every time), then measure a plain open.
        await unwrap(api.createStressProject(preset, path));
        const startedAt = performance.now();
        return { summary: await unwrap(api.openProject(path)), preset, startedAt };
      }),

    newProject: (path) =>
      opening('new', async () => ({
        summary: await unwrap(api.newProject(path ?? null)),
        preset: null,
      })),

    openProject: (path) =>
      opening('open', async () => ({
        summary: await unwrap(api.openProjectDialog(path ?? null)),
        preset: null,
      })),

    setCounters: (counters) => {
      set({ counters });
    },

    reload: async () => {
      if (!get().summary) return;
      try {
        const summary = await unwrap(api.getProjectSummary());
        const modules = await loadModules();
        const { canUndo, canRedo, activeModuleCount } = summary;
        set({ summary, modules, counters: { canUndo, canRedo, activeModuleCount } });
        useSelectionStore.getState().prune(summary);
      } catch (e) {
        set({ error: toCommandError(e) });
      }
    },
  };
});
