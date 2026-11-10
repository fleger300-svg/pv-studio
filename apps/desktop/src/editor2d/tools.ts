import type { PlanBuilding } from './planGeometry';
import { contains } from './planGeometry';
import {
  deleteVertex,
  ensureCcw,
  insertVertex,
  isSimple,
  moveVertex,
  nearestVertex,
  rectangle,
  translate,
  type EdgeChange,
} from './polygonEdit';
import { snap, snapStep, SNAP_PX, type Guide } from './snapping';
import type { Point } from './viewTransform';

/**
 * Plan tools as pure state machines: pointer and key events in, a draft (what to draw)
 * and, at the end of a gesture, a commit out. The plan view feeds them and turns commits
 * into commands; nothing here talks to Rust.
 */
export type ToolId = 'select' | 'rect' | 'polygon';

/** The footprint being edited or drawn (world points). */
export interface Draft {
  /** `null`: a new building. */
  buildingId: string | null;
  footprint: Point[];
  /** How the edges changed since the original footprint (per-edge roof data follows). */
  changes: EdgeChange[];
  /** A whole-building move (the footprint is the original one translated). */
  delta: Point | null;
  /** A polyline still being drawn (not closed). */
  open: boolean;
}

export interface ToolEnv {
  scale: number;
  buildings: readonly PlanBuilding[];
  selectedId: string | null;
}

export interface PointerInput {
  world: Point;
  /** Alt: no snapping. */
  alt: boolean;
}

/** What the plan draws for the tool. */
export interface ToolView {
  draft: Draft | null;
  guides: Guide[];
  /** Snapped cursor (drawing tools). */
  cursor: Point | null;
  /** Selected vertex of the selected building (editing). */
  vertex: number | null;
  /** Hover target, for the cursor shape. */
  hover: 'vertex' | 'insert' | 'move' | null;
  /** Translatable message of a refused gesture. */
  error: string | null;
}

export interface ToolResult {
  /** The tool used the event (the plan must not pan or select). */
  consumed: boolean;
  commit?: Draft;
}

export interface Tool {
  readonly id: ToolId;
  view(): ToolView;
  down(e: PointerInput, env: ToolEnv): ToolResult;
  move(e: PointerInput, env: ToolEnv): void;
  up(e: PointerInput, env: ToolEnv): ToolResult;
  doubleClick(e: PointerInput, env: ToolEnv): ToolResult;
  /** A key (Enter, Backspace, Delete, Escape); `consumed` stops the default shortcut. */
  key(key: string, env: ToolEnv): ToolResult;
}

const NOTHING: ToolResult = { consumed: false };
const MIN_SIDE_M = 1;

export const emptyView = (): ToolView => ({
  draft: null,
  guides: [],
  cursor: null,
  vertex: null,
  hover: null,
  error: null,
});

const allVertices = (env: ToolEnv) => env.buildings.flatMap((b) => b.footprint);

const direction = (a: Point | undefined, b: Point | undefined): number[] =>
  a && b && (a[0] !== b[0] || a[1] !== b[1]) ? [Math.atan2(b[1] - a[1], b[0] - a[0])] : [];

const newBuilding = (footprint: Point[]): Draft => ({
  buildingId: null,
  footprint,
  changes: [],
  delta: null,
  open: false,
});

/** Press-drag-release: an axis-aligned rectangle (at least 1 m a side). */
export class RectTool implements Tool {
  readonly id = 'rect';
  private start: Point | null = null;
  private state = emptyView();

  view(): ToolView {
    return this.state;
  }

  private snapped(e: PointerInput, env: ToolEnv) {
    return snap({ point: e.world, scale: env.scale, vertices: allVertices(env), disabled: e.alt });
  }

  down(e: PointerInput, env: ToolEnv): ToolResult {
    const s = this.snapped(e, env);
    this.start = s.point;
    this.state = { ...emptyView(), cursor: s.point, guides: s.guides };
    return { consumed: true };
  }

  move(e: PointerInput, env: ToolEnv): void {
    const s = this.snapped(e, env);
    const draft = this.start ? newBuilding(rectangle(this.start, s.point)) : null;
    this.state = { ...emptyView(), draft, cursor: s.point, guides: s.guides };
  }

  up(e: PointerInput, env: ToolEnv): ToolResult {
    const start = this.start;
    this.start = null;
    if (!start) return NOTHING;
    const end = this.snapped(e, env).point;
    this.state = emptyView();
    if (Math.abs(end[0] - start[0]) < MIN_SIDE_M || Math.abs(end[1] - start[1]) < MIN_SIDE_M) {
      return { consumed: true };
    }
    return { consumed: true, commit: newBuilding(rectangle(start, end)) };
  }

  doubleClick(): ToolResult {
    return NOTHING;
  }

  key(key: string): ToolResult {
    if (key !== 'Escape' || !this.start) return NOTHING;
    this.start = null;
    this.state = emptyView();
    return { consumed: true };
  }
}

/**
 * Click by click: each click adds a vertex; a click on the first vertex, a double click or
 * Enter closes; Backspace removes the last; Escape abandons.
 */
export class PolygonTool implements Tool {
  readonly id = 'polygon';
  private points: Point[] = [];
  private state = emptyView();

  view(): ToolView {
    return this.state;
  }

  private snapped(e: PointerInput, env: ToolEnv) {
    const last = this.points[this.points.length - 1];
    return snap({
      point: e.world,
      scale: env.scale,
      vertices: [...allVertices(env), ...this.points],
      anchor: last ?? null,
      directions: direction(this.points[this.points.length - 2], last),
      disabled: e.alt,
    });
  }

  private refresh(cursor: Point | null, guides: Guide[], error: string | null = null) {
    const draft: Draft | null =
      this.points.length > 0
        ? { ...newBuilding([...this.points, ...(cursor ? [cursor] : [])]), open: true }
        : null;
    this.state = { ...emptyView(), draft, cursor, guides, error };
  }

  private close(): ToolResult {
    if (this.points.length < 3) return { consumed: true };
    const { poly } = ensureCcw(this.points);
    if (!isSimple(poly)) {
      this.refresh(null, [], 'plan.errors.selfIntersecting');
      return { consumed: true };
    }
    this.points = [];
    this.state = emptyView();
    return { consumed: true, commit: newBuilding(poly) };
  }

  down(e: PointerInput, env: ToolEnv): ToolResult {
    const s = this.snapped(e, env);
    const first = this.points[0];
    const last = this.points[this.points.length - 1];
    const tol = SNAP_PX / env.scale;
    if (
      first &&
      this.points.length >= 3 &&
      Math.hypot(s.point[0] - first[0], s.point[1] - first[1]) <= tol
    ) {
      return this.close();
    }
    // The second click of a double click lands on the last point: no duplicate.
    if (!last || Math.hypot(s.point[0] - last[0], s.point[1] - last[1]) > 1e-9) {
      this.points.push(s.point);
    }
    this.refresh(s.point, s.guides);
    return { consumed: true };
  }

  move(e: PointerInput, env: ToolEnv): void {
    const s = this.snapped(e, env);
    this.refresh(s.point, s.guides, this.state.error);
  }

  up(): ToolResult {
    return { consumed: this.points.length > 0 };
  }

  doubleClick(): ToolResult {
    return this.close();
  }

  key(key: string): ToolResult {
    if (this.points.length === 0) return NOTHING;
    if (key === 'Enter') return this.close();
    if (key === 'Backspace') {
      this.points.pop();
      this.refresh(this.state.cursor, []);
      return { consumed: true };
    }
    if (key === 'Escape') {
      this.points = [];
      this.state = emptyView();
      return { consumed: true };
    }
    return NOTHING;
  }
}

type Drag =
  | {
      kind: 'vertex';
      buildingId: string;
      index: number;
      poly: Point[];
      changes: EdgeChange[];
      moved: boolean;
    }
  | { kind: 'move'; buildingId: string; poly: Point[]; start: Point; delta: Point };

/**
 * Editing the selected building: drag a vertex, drag the « + » in the middle of an edge to
 * add one, drag the inside to move the building; Delete removes the selected vertex.
 */
export class SelectTool implements Tool {
  readonly id = 'select';
  private drag: Drag | null = null;
  private vertex: { buildingId: string; index: number } | null = null;
  private state = emptyView();

  view(): ToolView {
    return this.state;
  }

  private selected(env: ToolEnv): PlanBuilding | null {
    return env.buildings.find((b) => b.id === env.selectedId) ?? null;
  }

  /** What is under the pointer on the selected building. */
  private target(world: Point, env: ToolEnv) {
    const b = this.selected(env);
    if (!b) return null;
    const tol = SNAP_PX / env.scale;
    const vi = nearestVertex(b.footprint, world, tol);
    if (vi >= 0) return { kind: 'vertex' as const, b, index: vi };
    const n = b.footprint.length;
    for (let i = 0; i < n; i++) {
      const a = b.footprint[i];
      const c = b.footprint[(i + 1) % n];
      if (!a || !c) continue;
      const mid: Point = [(a[0] + c[0]) / 2, (a[1] + c[1]) / 2];
      if (Math.hypot(mid[0] - world[0], mid[1] - world[1]) <= tol) {
        return { kind: 'insert' as const, b, index: i, at: mid };
      }
    }
    if (contains(b.footprint, world)) return { kind: 'move' as const, b };
    return null;
  }

  private selectedVertex(env: ToolEnv): number | null {
    return this.vertex && this.vertex.buildingId === env.selectedId ? this.vertex.index : null;
  }

  down(e: PointerInput, env: ToolEnv): ToolResult {
    const t = this.target(e.world, env);
    if (!t) {
      this.vertex = null;
      this.state = emptyView();
      return NOTHING;
    }
    if (t.kind === 'vertex') {
      this.vertex = { buildingId: t.b.id, index: t.index };
      this.drag = {
        kind: 'vertex',
        buildingId: t.b.id,
        index: t.index,
        poly: [...t.b.footprint],
        changes: [],
        moved: false,
      };
    } else if (t.kind === 'insert') {
      const poly = insertVertex(t.b.footprint, t.index, t.at);
      this.vertex = { buildingId: t.b.id, index: t.index + 1 };
      this.drag = {
        kind: 'vertex',
        buildingId: t.b.id,
        index: t.index + 1,
        poly,
        changes: [{ kind: 'insert', edge: t.index }],
        moved: false,
      };
    } else {
      this.vertex = null;
      this.drag = {
        kind: 'move',
        buildingId: t.b.id,
        poly: [...t.b.footprint],
        start: e.world,
        delta: [0, 0],
      };
    }
    this.state = { ...emptyView(), vertex: this.selectedVertex(env), hover: t.kind };
    return { consumed: true };
  }

  move(e: PointerInput, env: ToolEnv): void {
    const d = this.drag;
    if (!d) {
      const t = this.target(e.world, env);
      this.state = { ...this.state, hover: t?.kind ?? null, vertex: this.selectedVertex(env) };
      return;
    }
    if (d.kind === 'vertex') {
      const n = d.poly.length;
      const prev = d.poly[(d.index + n - 1) % n];
      const s = snap({
        point: e.world,
        scale: env.scale,
        vertices: [
          ...env.buildings.filter((b) => b.id !== d.buildingId).flatMap((b) => b.footprint),
          ...d.poly.filter((_, i) => i !== d.index),
        ],
        anchor: prev ?? null,
        directions: [
          ...direction(d.poly[(d.index + n - 2) % n], prev),
          ...direction(d.poly[(d.index + 1) % n], d.poly[(d.index + 2) % n]),
        ],
        disabled: e.alt,
      });
      d.poly = moveVertex(d.poly, d.index, s.point);
      d.moved = true;
      this.state = {
        ...emptyView(),
        draft: {
          buildingId: d.buildingId,
          footprint: d.poly,
          changes: d.changes,
          delta: null,
          open: false,
        },
        guides: s.guides,
        vertex: d.index,
        hover: 'vertex',
      };
    } else {
      const step = e.alt ? 0 : snapStep(env.scale);
      const r = (x: number) => (step > 0 ? Math.round(x / step) * step : x);
      d.delta = [r(e.world[0] - d.start[0]), r(e.world[1] - d.start[1])];
      this.state = {
        ...emptyView(),
        draft: {
          buildingId: d.buildingId,
          footprint: translate(d.poly, d.delta[0], d.delta[1]),
          changes: [],
          delta: d.delta,
          open: false,
        },
        hover: 'move',
      };
    }
  }

  up(_e: PointerInput, env: ToolEnv): ToolResult {
    const d = this.drag;
    this.drag = null;
    if (!d) return NOTHING;
    const done = (commit?: Draft): ToolResult => {
      this.state = { ...emptyView(), vertex: this.selectedVertex(env), error: this.state.error };
      return commit ? { consumed: true, commit } : { consumed: true };
    };
    if (d.kind === 'move') {
      if (d.delta[0] === 0 && d.delta[1] === 0) return done();
      return done({
        buildingId: d.buildingId,
        footprint: translate(d.poly, d.delta[0], d.delta[1]),
        changes: [],
        delta: d.delta,
        open: false,
      });
    }
    if (!d.moved) return done();
    return done(this.shape(d.buildingId, d.poly, d.changes));
  }

  /** A changed footprint, counter-clockwise and simple, or `undefined` (refused). */
  private shape(buildingId: string, poly: Point[], changes: EdgeChange[]): Draft | undefined {
    if (!isSimple(poly)) {
      this.state = { ...this.state, error: 'plan.errors.selfIntersecting' };
      return undefined;
    }
    const ccw = ensureCcw(poly);
    this.state = { ...this.state, error: null };
    return {
      buildingId,
      footprint: ccw.poly,
      changes: [...changes, ccw.change],
      delta: null,
      open: false,
    };
  }

  doubleClick(): ToolResult {
    return NOTHING;
  }

  key(key: string, env: ToolEnv): ToolResult {
    if (key === 'Escape' && (this.drag || this.vertex)) {
      this.drag = null;
      this.vertex = null;
      this.state = emptyView();
      return { consumed: true };
    }
    const index = this.selectedVertex(env);
    const b = this.selected(env);
    if ((key === 'Delete' || key === 'Backspace') && index !== null && b) {
      const poly = deleteVertex(b.footprint, index);
      this.vertex = null;
      this.state = emptyView();
      if (!poly) return { consumed: true };
      const commit = this.shape(b.id, poly, [{ kind: 'remove', vertex: index }]);
      return commit ? { consumed: true, commit } : { consumed: true };
    }
    return NOTHING;
  }
}

export function createTool(id: ToolId): Tool {
  switch (id) {
    case 'rect':
      return new RectTool();
    case 'polygon':
      return new PolygonTool();
    case 'select':
      return new SelectTool();
  }
}
