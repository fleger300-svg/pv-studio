import { useEffect } from 'react';
import { events } from '../bindings';
import { api, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useStaticsStore } from '../store/statics';

/**
 * Keeps the statics store in sync with the compute thread: listens to `result:ready`,
 * `compute:state` and `compute:failed`, and loads the snapshot when a project is opened (the
 * first results may have been emitted before the listeners were attached). Mount once.
 */
export function useStaticsSync(): void {
  const projectId = useProjectStore((s) => s.summary?.projectId ?? null);

  useEffect(() => {
    const store = useStaticsStore.getState();
    store.clear();
    let disposed = false;
    // Read through a call: TypeScript would narrow the flag across `await`.
    const isDisposed = () => disposed;
    const unlisten: (() => void)[] = [];
    void (async () => {
      const fns = await Promise.all([
        events.resultReady.listen((e) => {
          useStaticsStore.getState().applyReady(e.payload);
        }),
        events.computeState.listen((e) => {
          useStaticsStore.getState().applyState(e.payload.busy, e.payload.seq);
        }),
        events.computeFailed.listen((e) => {
          useStaticsStore.getState().applyFailure(e.payload.error);
        }),
      ]);
      if (isDisposed()) {
        for (const f of fns) f();
        return;
      }
      unlisten.push(...fns);
      if (projectId) {
        try {
          const snapshot = await unwrap(api.getStaticsSnapshot());
          if (!isDisposed()) useStaticsStore.getState().applySnapshot(snapshot);
        } catch {
          // No project open on the Rust side (yet): the events will fill the store.
        }
      }
    })();
    return () => {
      disposed = true;
      for (const f of unlisten) f();
    };
  }, [projectId]);
}
