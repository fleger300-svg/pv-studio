/// Box-selection worker: keeps a copy of the module centres and answers rectangle queries.
import { selectInRect, type NdcRect } from './selectInRect';

export type BoxSelectRequest =
  | { type: 'init'; positions: Float32Array; removed: Uint8Array }
  | { type: 'update'; ids: Uint32Array; positions: Float32Array; removed: Uint8Array }
  | { type: 'query'; requestId: number; viewProj: Float32Array; rect: NdcRect };

export interface BoxSelectResponse {
  requestId: number;
  ids: Uint32Array;
}

let positions: Float32Array = new Float32Array(0);
let removed: Uint8Array = new Uint8Array(0);

self.onmessage = (event: MessageEvent<BoxSelectRequest>) => {
  const msg = event.data;
  switch (msg.type) {
    case 'init':
      positions = msg.positions;
      removed = msg.removed;
      break;
    case 'update':
      msg.ids.forEach((id, i) => {
        positions.set(msg.positions.subarray(i * 3, i * 3 + 3), id * 3);
        removed[id] = msg.removed[i] ?? 0;
      });
      break;
    case 'query': {
      const ids = selectInRect(
        positions,
        removed.length,
        msg.viewProj,
        msg.rect,
        (id) => removed[id] === 1,
      );
      const response: BoxSelectResponse = { requestId: msg.requestId, ids };
      self.postMessage(response, { transfer: [ids.buffer] });
      break;
    }
  }
};
