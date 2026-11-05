import { useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { useProjectStore } from '../store/project';
import { usePerfStore } from './perfStore';
import { frameRecorder } from './frames';

/**
 * Lives inside the Canvas (one instance per opened project). Records frame timestamps and
 * renderer counters, measures project-open → first rendered frame, and drives benchmark
 * frame tasks (invalidating every frame while a task runs).
 */
export function FrameProbe() {
  const first = useRef(true);
  useFrame((state) => {
    const now = performance.now();
    // gl.info holds the previous frame's counters (it is reset at the start of each render).
    const { calls, triangles } = state.gl.info.render;
    frameRecorder.record(now, calls, triangles);
    if (first.current) {
      first.current = false;
      const started = useProjectStore.getState().openStartedAt;
      // setTimeout(0) runs right after this render call returns.
      if (started !== null) {
        setTimeout(() => {
          usePerfStore.getState().setOpenMs(performance.now() - started);
        }, 0);
      }
    }
    if (frameRecorder.task?.(now)) state.invalidate();
  });
  return null;
}
