import { Vector3 } from 'three';
import type { ApplyResult, Command, CommandError, ProjectSummary } from '../bindings';
import { events } from '../bindings';
import { api, BackendError, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useToastStore } from '../store/toast';
import { useViewStore } from '../store/view';
import { BoxSelectClient } from '../workers/boxSelectClient';
import type { NdcRect } from '../workers/selectInRect';
import type { GridIndex } from './gridIndex';
import { decodeModuleBuffer } from './moduleBuffer';
import {
  DEFAULT_CELL_COLOR,
  SELECTED_COLOR,
  STRING_PALETTE,
  type ColorMode,
  type PanelMeshes,
} from './PanelMeshes';
import { vec3 } from './frames';

/** Timing of one command round trip, reported to the performance HUD. */
export interface CommandTiming {
  kind: 'apply' | 'undo' | 'redo';
  /** Front-measured invoke → result, ms. */
  roundTripMs: number;
  /** Time spent in Rust, µs (from `ApplyResult`). */
  rustUs: number;
}

/** Drag offsets below this (m) are treated as a click, not a move. */
const MIN_MOVE_M = 0.005;

/**
 * Imperative editing layer of the viewport, outside React. Selection, local drag preview
 * (GPU only — no IPC until pointer-up), command round trips, rollback on rejection, and
 * resync with Rust on `model:changed`. The model itself stays in Rust.
 */
export class Editor {
  readonly selection = new Set<number>();
  onCommand: ((t: CommandTiming) => void) | null = null;
  private drag: { ids: number[]; startU: number; startV: number; du: number; dv: number } | null =
    null;
  private busy = false;
  private colorMode: ColorMode = 'default';
  private readonly axes: { u: Vector3; v: Vector3 }[];
  private readonly scratch = new Vector3();
  private unlisten: (() => void) | null = null;
  private box: BoxSelectClient | null = null;
  private attached = false;

  constructor(
    readonly summary: ProjectSummary,
    readonly panels: PanelMeshes,
    readonly grid: GridIndex,
    private readonly invalidate: () => void,
  ) {
    this.axes = summary.arrays.map((a) => {
      const frame = summary.roofFaces[a.roofFaceIndex]?.frame;
      return frame
        ? { u: vec3(frame.axisU), v: vec3(frame.axisV) }
        : { u: new Vector3(), v: new Vector3() };
    });
  }

  /** Starts the box-selection worker and listens to `model:changed`. Idempotent with `detach`. */
  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    this.box = new BoxSelectClient(this.panels.data);
    const unlisten = await events.modelChanged.listen((e) => {
      const { canUndo, canRedo, moduleIds } = e.payload;
      const store = useProjectStore.getState();
      store.setCounters({ ...store.counters, canUndo, canRedo });
      void this.sync(moduleIds);
    });
    // `detach()` may have run while awaiting.
    if (this.isAttached()) this.unlisten = unlisten;
    else unlisten();
  }

  /** Read through a call so TypeScript does not narrow it across `await`. */
  private isAttached(): boolean {
    return this.attached;
  }

  detach(): void {
    this.attached = false;
    this.unlisten?.();
    this.unlisten = null;
    this.box?.dispose();
    this.box = null;
  }

  // ---- colours & selection --------------------------------------------------------------

  readonly colorOf = (id: number) => {
    if (this.selection.has(id)) return SELECTED_COLOR;
    if (this.colorMode === 'string') {
      return (
        STRING_PALETTE[this.panels.stringIndex(id) % STRING_PALETTE.length] ?? DEFAULT_CELL_COLOR
      );
    }
    return DEFAULT_CELL_COLOR;
  };

  setColorMode(mode: ColorMode): void {
    this.colorMode = mode;
    this.panels.updateAllColors(this.colorOf);
    this.invalidate();
  }

  private publishSelection(changed: Iterable<number>): void {
    this.panels.updateColors(changed, this.colorOf);
    useViewStore.setState({ selectionCount: this.selection.size });
    this.invalidate();
  }

  /** Replaces (or extends, when `additive`) the selection. */
  select(ids: Iterable<number>, additive = false): void {
    const changed = new Set<number>();
    if (!additive) {
      for (const id of this.selection) changed.add(id);
      this.selection.clear();
    }
    for (const id of ids) {
      if (!this.selection.has(id)) {
        this.selection.add(id);
        changed.add(id);
      } else if (!additive) {
        changed.add(id);
      }
    }
    this.publishSelection(changed);
  }

  /** Shift-click behaviour: adds the module, or removes it if already selected. */
  toggle(id: number): void {
    if (this.selection.has(id)) this.selection.delete(id);
    else this.selection.add(id);
    this.publishSelection([id]);
  }

  clearSelection(): void {
    if (this.selection.size === 0) return;
    const changed = Array.from(this.selection);
    this.selection.clear();
    this.publishSelection(changed);
  }

  /** Module under roof point `(u, v)` of a face (grid lookup). */
  pick(faceIndex: number, u: number, v: number): number | null {
    return this.grid.hit(faceIndex, u, v);
  }

  /** Box selection through the worker. */
  async boxSelect(viewProj: ArrayLike<number>, rect: NdcRect, additive: boolean): Promise<number> {
    if (!this.box) return 0;
    const ids = await this.box.query(viewProj, rect);
    this.select(ids, additive);
    return ids.length;
  }

  // ---- drag move (local only) --------------------------------------------------------------

  get dragging(): boolean {
    return this.drag !== null;
  }

  /** Starts a drag of the current selection at roof point `(u, v)`. */
  beginDrag(u: number, v: number): boolean {
    if (this.busy || this.selection.size === 0) return false;
    this.drag = { ids: Array.from(this.selection), startU: u, startV: v, du: 0, dv: 0 };
    return true;
  }

  /** Pointer moved to roof point `(u, v)` (of the face where the drag started). */
  dragTo(u: number, v: number): void {
    if (!this.drag) return;
    this.previewMove(u - this.drag.startU, v - this.drag.startV);
  }

  /**
   * Moves the dragged instances by `(du, dv)` in their own roof frames — matrix updates of
   * the touched instances only. Nothing crosses the IPC here.
   */
  previewMove(du: number, dv: number): void {
    if (!this.drag) return;
    this.drag.du = du;
    this.drag.dv = dv;
    this.panels.updateTransforms(this.drag.ids, (id) => {
      const axes = this.axes[this.panels.data.arrayIndex[id] ?? 0];
      if (!axes) return undefined;
      return this.scratch.copy(axes.u).multiplyScalar(du).addScaledVector(axes.v, dv);
    });
    this.invalidate();
  }

  /** Pointer released: sends exactly one `MoveModules`, or rolls back if Rust rejects it. */
  async endDrag(): Promise<void> {
    const drag = this.drag;
    this.drag = null;
    if (!drag) return;
    if (Math.hypot(drag.du, drag.dv) < MIN_MOVE_M) {
      this.rollback(drag.ids);
      return;
    }
    const ok = await this.run('apply', {
      type: 'MoveModules',
      ids: drag.ids,
      du: drag.du,
      dv: drag.dv,
    });
    // On success the preview stays until `model:changed` resyncs the exact positions.
    if (!ok) this.rollback(drag.ids);
  }

  cancelDrag(): void {
    if (this.drag) this.rollback(this.drag.ids);
    this.drag = null;
  }

  private rollback(ids: readonly number[]): void {
    this.panels.updateTransforms(ids);
    this.invalidate();
  }

  // ---- commands ---------------------------------------------------------------------------

  async removeSelected(): Promise<void> {
    if (this.selection.size === 0) return;
    const ids = Array.from(this.selection);
    if (await this.run('apply', { type: 'RemoveModules', ids })) this.clearSelection();
  }

  undo(): Promise<boolean> {
    return this.run('undo');
  }

  redo(): Promise<boolean> {
    return this.run('redo');
  }

  /** Applies a command (or undo/redo), times the round trip and reports errors as toasts. */
  async run(kind: CommandTiming['kind'], cmd?: Command): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    const started = performance.now();
    try {
      let result: ApplyResult;
      if (kind === 'apply' && cmd) result = await unwrap(api.applyCommand(cmd));
      else if (kind === 'undo') result = await unwrap(api.undo());
      else result = await unwrap(api.redo());
      this.onCommand?.({ kind, roundTripMs: performance.now() - started, rustUs: result.rustUs });
      useProjectStore.getState().setCounters({
        canUndo: result.canUndo,
        canRedo: result.canRedo,
        activeModuleCount: result.activeModuleCount,
      });
      return true;
    } catch (e) {
      const message: CommandError =
        e instanceof BackendError
          ? { key: e.key, params: e.params }
          : { key: 'errors.store.failure', params: { detail: String(e) } };
      useToastStore.getState().push(message);
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** Pulls the exact state of changed modules from Rust (binary) and updates the GPU. */
  async sync(ids: readonly number[]): Promise<void> {
    if (ids.length === 0 || !this.attached) return;
    const buffer =
      ids.length > this.panels.data.count / 2
        ? await unwrap(api.getModuleBuffer())
        : await unwrap(api.getModuleBufferFor([...ids]));
    if (!this.isAttached()) return;
    const changed = this.panels.applySubset(decodeModuleBuffer(buffer));
    this.grid.update(changed);
    this.box?.update(this.panels.data, changed);
    this.panels.updateColors(changed, this.colorOf);
    this.invalidate();
  }
}
