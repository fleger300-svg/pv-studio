import { FLAG_REMOVED, type ModuleData } from '../viewport3d/moduleBuffer';
import type { BoxSelectRequest, BoxSelectResponse } from './boxSelect.worker';
import type { NdcRect } from './selectInRect';

/** Main-thread side of the box-selection worker. */
export class BoxSelectClient {
  private readonly worker = new Worker(new URL('./boxSelect.worker.ts', import.meta.url), {
    type: 'module',
  });
  private nextId = 1;
  private readonly pending = new Map<number, (ids: Uint32Array) => void>();

  constructor(data: ModuleData) {
    this.worker.onmessage = (e: MessageEvent<BoxSelectResponse>) => {
      this.pending.get(e.data.requestId)?.(e.data.ids);
      this.pending.delete(e.data.requestId);
    };
    // Sent once, as transferables (copies, so the renderer keeps its arrays).
    const positions = data.positions.slice();
    const removed = Uint8Array.from(data.flags, (f) => (f & FLAG_REMOVED ? 1 : 0));
    this.post({ type: 'init', positions, removed }, [positions.buffer, removed.buffer]);
  }

  private post(msg: BoxSelectRequest, transfer: Transferable[] = []): void {
    this.worker.postMessage(msg, { transfer });
  }

  /** Sends the new centres/flags of the given modules after a change. */
  update(data: ModuleData, ids: readonly number[]): void {
    const positions = new Float32Array(ids.length * 3);
    const removed = new Uint8Array(ids.length);
    ids.forEach((id, i) => {
      positions.set(data.positions.subarray(id * 3, id * 3 + 3), i * 3);
      removed[i] = (data.flags[id] ?? 0) & FLAG_REMOVED ? 1 : 0;
    });
    const idArray = Uint32Array.from(ids);
    this.post({ type: 'update', ids: idArray, positions, removed }, [
      idArray.buffer,
      positions.buffer,
      removed.buffer,
    ]);
  }

  /** Ids whose projected centre falls inside `rect`. */
  query(viewProj: ArrayLike<number>, rect: NdcRect): Promise<Uint32Array> {
    const requestId = this.nextId++;
    const matrix = Float32Array.from(viewProj);
    return new Promise((resolve) => {
      this.pending.set(requestId, resolve);
      this.post({ type: 'query', requestId, viewProj: matrix, rect }, [matrix.buffer]);
    });
  }

  dispose(): void {
    this.worker.terminate();
    this.pending.clear();
  }
}
