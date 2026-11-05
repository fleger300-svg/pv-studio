import {
  Color,
  DynamicDrawUsage,
  Group,
  InstancedMesh,
  Matrix4,
  Quaternion,
  Vector3,
  type BufferGeometry,
} from 'three';
import { tokens } from '@pv/ui';
import type { ProjectSummary } from '../bindings';
import { FLAG_REMOVED, type ModuleData } from './moduleBuffer';
import { coalesceRuns } from './ranges';
import { createModuleGeometry, createPanelMaterial } from './panelMaterial';

export type ColorMode = 'default' | 'string';

export const DEFAULT_CELL_COLOR = new Color('#1f3b73');
export const SELECTED_COLOR = new Color(tokens.color.selection3d);
export const STRING_PALETTE: readonly Color[] = [
  '#2e86ab',
  '#a23b72',
  '#3bb273',
  '#e1bc29',
  '#7768ae',
  '#00a6a6',
  '#d1495b',
  '#8c6d46',
].map((hex) => new Color(hex));

/** Colour of one module for the current view state. */
export type ColorOf = (id: number) => Color;

const ZERO = new Matrix4().makeScale(0, 0, 0);
const UNIT = new Vector3(1, 1, 1);

/**
 * GPU side of the panels: **one `InstancedMesh` per array**, driven by the decoded PVS1
 * typed arrays. Edits update only the touched instance ranges (`addUpdateRange`); meshes are
 * never rebuilt. Instance index = module id − array's first module id.
 */
export class PanelMeshes {
  readonly group = new Group();
  readonly meshes: InstancedMesh[] = [];
  private readonly material = createPanelMaterial();
  private readonly geometries: BufferGeometry[] = [];
  private readonly firstIds: number[];
  private readonly m = new Matrix4();
  private readonly p = new Vector3();
  private readonly q = new Quaternion();

  constructor(
    private readonly summary: ProjectSummary,
    readonly data: ModuleData,
  ) {
    this.firstIds = summary.arrays.map((a) => a.firstModuleId);
    for (const a of summary.arrays) {
      const [sx, sy, sz] = a.moduleSize;
      const geometry = createModuleGeometry(sx, sy, sz);
      this.geometries.push(geometry);
      const mesh = new InstancedMesh(geometry, this.material, a.moduleCount);
      mesh.name = `panels:${a.array.id}`;
      mesh.instanceMatrix.setUsage(DynamicDrawUsage);
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      // Instances span whole roofs and move during drags; culling buys nothing here.
      mesh.frustumCulled = false;
      if (a.moduleCount > 0) {
        mesh.setColorAt(0, DEFAULT_CELL_COLOR);
        mesh.instanceColor?.setUsage(DynamicDrawUsage);
      }
      this.meshes.push(mesh);
      this.group.add(mesh);
    }
    for (let id = 0; id < data.count; id++) this.writeMatrix(id);
    for (const mesh of this.meshes) mesh.instanceMatrix.needsUpdate = true;
  }

  /** Mesh index and instance index of a module id. */
  locate(id: number): { mesh: number; local: number } {
    const mesh = this.data.arrayIndex[id] ?? 0;
    return { mesh, local: id - (this.firstIds[mesh] ?? 0) };
  }

  /** Total instances (for the HUD). */
  get instanceCount(): number {
    return this.data.count;
  }

  private writeMatrix(id: number, offset?: Vector3): void {
    const { mesh, local } = this.locate(id);
    const target = this.meshes[mesh];
    if (!target) return;
    if ((this.data.flags[id] ?? 0) & FLAG_REMOVED) {
      target.setMatrixAt(local, ZERO);
      return;
    }
    const pos = this.data.positions;
    const rot = this.data.rotations;
    this.p.set(pos[id * 3] ?? 0, pos[id * 3 + 1] ?? 0, pos[id * 3 + 2] ?? 0);
    if (offset) this.p.add(offset);
    this.q.set(rot[id * 4] ?? 0, rot[id * 4 + 1] ?? 0, rot[id * 4 + 2] ?? 0, rot[id * 4 + 3] ?? 1);
    target.setMatrixAt(local, this.m.compose(this.p, this.q, UNIT));
  }

  /** Groups ids per mesh and uploads only the touched ranges of `attribute`. */
  private upload(ids: Iterable<number>, which: 'matrix' | 'color'): void {
    const perMesh = new Map<number, number[]>();
    for (const id of ids) {
      const { mesh, local } = this.locate(id);
      let list = perMesh.get(mesh);
      if (!list) perMesh.set(mesh, (list = []));
      list.push(local);
    }
    for (const [index, locals] of perMesh) {
      const mesh = this.meshes[index];
      const attribute = which === 'matrix' ? mesh?.instanceMatrix : mesh?.instanceColor;
      if (!attribute) continue;
      attribute.clearUpdateRanges();
      for (const run of coalesceRuns(locals)) {
        attribute.addUpdateRange(run.start * attribute.itemSize, run.count * attribute.itemSize);
      }
      attribute.needsUpdate = true;
    }
  }

  /** Re-writes the matrices of `ids` from `data` (optionally displaced by `offsetOf`). */
  updateTransforms(ids: Iterable<number>, offsetOf?: (id: number) => Vector3 | undefined): void {
    const list = Array.from(ids);
    for (const id of list) this.writeMatrix(id, offsetOf?.(id));
    this.upload(list, 'matrix');
  }

  /** Copies a partial PVS1 buffer (e.g. after undo) into `data` and updates those instances. */
  applySubset(subset: ModuleData): number[] {
    const ids = Array.from(subset.ids);
    subset.ids.forEach((id, i) => {
      this.data.flags[id] = subset.flags[i] ?? 0;
      this.data.positions.set(subset.positions.subarray(i * 3, i * 3 + 3), id * 3);
      this.data.rotations.set(subset.rotations.subarray(i * 4, i * 4 + 4), id * 4);
    });
    this.updateTransforms(ids);
    return ids;
  }

  /** Recolours the given modules only. */
  updateColors(ids: Iterable<number>, colorOf: ColorOf): void {
    const list = Array.from(ids);
    for (const id of list) {
      const { mesh, local } = this.locate(id);
      this.meshes[mesh]?.setColorAt(local, colorOf(id));
    }
    this.upload(list, 'color');
  }

  /** Recolours every module (colour mode change). */
  updateAllColors(colorOf: ColorOf): void {
    for (let id = 0; id < this.data.count; id++) {
      const { mesh, local } = this.locate(id);
      this.meshes[mesh]?.setColorAt(local, colorOf(id));
    }
    for (const mesh of this.meshes) {
      if (!mesh.instanceColor) continue;
      mesh.instanceColor.clearUpdateRanges();
      mesh.instanceColor.needsUpdate = true;
    }
  }

  /** Index of the module's fake string (same rule as the Rust generator). */
  stringIndex(id: number): number {
    return Math.floor(id / this.summary.modulesPerString);
  }

  dispose(): void {
    for (const g of this.geometries) g.dispose();
    this.material.dispose();
    for (const mesh of this.meshes) mesh.dispose();
  }
}
