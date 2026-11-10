import type { RootState } from '@react-three/fiber';
import type { usePlanStore } from './store/plan';
import type { useProjectStore } from './store/project';
import type { useSelectionStore } from './store/selection';
import type { useWorkflowStore } from './store/workflow';
import type { Editor } from './viewport3d/editor';

declare global {
  interface Window {
    /** Dev-only automation handle (set by the viewport when `import.meta.env.DEV`). */
    __pv?: {
      editor: Editor;
      three: () => RootState;
      ipcCallCount: () => number;
      project: typeof useProjectStore;
      selection: typeof useSelectionStore;
      workflow: typeof useWorkflowStore;
      plan: typeof usePlanStore;
    };
  }
}
