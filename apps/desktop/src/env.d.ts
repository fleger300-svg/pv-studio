import type { RootState } from '@react-three/fiber';
import type { Editor } from './viewport3d/editor';

declare global {
  interface Window {
    /** Dev-only automation handle (set by the viewport when `import.meta.env.DEV`). */
    __pv?: { editor: Editor; three: () => RootState; ipcCallCount: () => number };
  }
}
