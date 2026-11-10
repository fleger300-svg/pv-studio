import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Circle, Group, Layer, Line, Rect, Stage, Text } from 'react-konva';
import { IconButton } from '@pv/ui';
import type {
  Building,
  CommandError,
  ProjectSummary,
  RoofEdgeKind,
  RoofPreview,
} from '../bindings';
import { toCommandError } from '../commands/runCommand';
import { api, unwrap } from '../ipc';
import { IconFit, IconPolygon, IconRect, IconSelect } from '../shell/icons';
import { usePlanStore } from '../store/plan';
import { useSelectionStore } from '../store/selection';
import { useUiStore } from '../store/ui';
import { commitDraft, draftBuilding, ensureSpec, nextBuildingName } from './buildingCommands';
import { LatestOnly } from './latestOnly';
import {
  centroid,
  contains,
  errorEdges,
  planBounds,
  planBuildings,
  planFaces,
  type PlanBuilding,
  type PlanEdge,
  type PlanFace,
} from './planGeometry';
import { ensureCcw, isSimple, setEdgeLength } from './polygonEdit';
import {
  createTool,
  emptyView,
  type Draft,
  type Tool,
  type ToolEnv,
  type ToolId,
  type ToolView,
} from './tools';
import {
  fit,
  gridStep,
  pan,
  toScreen,
  toWorld,
  visibleBounds,
  zoomAt,
  type Point,
  type ViewTransform,
} from './viewTransform';

/** Pointer travel (px) below which a press/release is a click, not a pan. */
const CLICK_TOLERANCE_PX = 4;
/** An empty site is shown as 40 m around the origin. */
const EMPTY_SITE = { minE: -20, minN: -20, maxE: 20, maxN: 20 };
const HANDLE_PX = 7;

interface PlanColors {
  grid: string;
  gridMajor: string;
  footprint: string;
  footprintStroke: string;
  roof: string;
  text: string;
  accent: string;
  error: string;
  edge: Record<RoofEdgeKind, string>;
}

/** Plan colours, from the theme tokens (re-read when the theme changes). */
function usePlanColors(): PlanColors {
  const theme = useUiStore((s) => s.theme);
  return useMemo(() => {
    const css = getComputedStyle(document.documentElement);
    const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
    const text = v('--color-text', '#1b1b1f');
    const muted = v('--color-text-muted', '#5e5e66');
    const dark = theme === 'dark';
    return {
      grid: dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.06)',
      gridMajor: dark ? 'rgba(255,255,255,0.14)' : 'rgba(0,0,0,0.14)',
      footprint: v('--color-surface', '#ffffff'),
      footprintStroke: muted,
      roof: dark ? 'rgba(201,120,96,0.35)' : 'rgba(141,90,74,0.22)',
      text,
      accent: v('--color-accent', '#ff6727'),
      error: dark ? '#ff6b6b' : '#d32f2f',
      edge: {
        eave: muted,
        verge: muted,
        parapet: muted,
        ridge: dark ? '#e0a090' : '#8d5a4a',
        hip: dark ? '#e0a090' : '#8d5a4a',
        valley: dark ? '#7fb0ff' : '#2f6fbf',
        abutment: muted,
        other: muted,
      },
    };
  }, [theme]);
}

/** Size of an element, kept up to date. */
function useElementSize(ref: React.RefObject<HTMLElement | null>): { w: number; h: number } {
  const [size, setSize] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
    };
  }, [ref]);
  return size;
}

/** What is under a plan point: a roof face first, else a building footprint. */
export function pick(
  faces: PlanFace[],
  buildings: PlanBuilding[],
  p: readonly [number, number],
): { faceId: string | null; buildingId: string } | null {
  const face = faces.find((f) => contains(f.polygon, p));
  if (face) return { faceId: face.id, buildingId: face.buildingId };
  const b = buildings.find((x) => contains(x.footprint, p));
  return b ? { faceId: null, buildingId: b.id } : null;
}

function GridLines({
  t,
  w,
  h,
  colors,
}: {
  t: ViewTransform;
  w: number;
  h: number;
  colors: PlanColors;
}) {
  const step = gridStep(t.scale);
  const major = step * 5;
  const vb = visibleBounds(t, w, h);
  const lines: React.JSX.Element[] = [];
  for (let e = Math.ceil(vb.minE / step) * step; e <= vb.maxE; e += step) {
    const [x] = toScreen(t, [e, 0]);
    const isMajor = Math.abs(e / major - Math.round(e / major)) < 1e-6;
    lines.push(
      <Line
        key={`e${e.toFixed(3)}`}
        points={[x, 0, x, h]}
        stroke={isMajor ? colors.gridMajor : colors.grid}
        strokeWidth={1}
      />,
    );
  }
  for (let n = Math.ceil(vb.minN / step) * step; n <= vb.maxN; n += step) {
    const [, y] = toScreen(t, [0, n]);
    const isMajor = Math.abs(n / major - Math.round(n / major)) < 1e-6;
    lines.push(
      <Line
        key={`n${n.toFixed(3)}`}
        points={[0, y, w, y]}
        stroke={isMajor ? colors.gridMajor : colors.grid}
        strokeWidth={1}
      />,
    );
  }
  return <>{lines}</>;
}

const flat = (pts: readonly (readonly [number, number])[]) => pts.flatMap(([e, n]) => [e, n]);

function edgesOf(polygon: Point[], kinds: RoofEdgeKind[]): PlanEdge[] {
  return polygon.map((a, i) => ({
    a,
    b: polygon[(i + 1) % polygon.length] ?? a,
    kind: kinds[i] ?? 'other',
  }));
}

/** A dimension label of the selected (or drafted) footprint, in screen pixels. */
interface Dimension {
  edge: number;
  x: number;
  y: number;
  text: string;
  value: string;
}

const TOOLS: readonly { id: ToolId; icon: () => React.JSX.Element; shortcut: string }[] = [
  { id: 'select', icon: IconSelect, shortcut: 'v' },
  { id: 'rect', icon: IconRect, shortcut: 'r' },
  { id: 'polygon', icon: IconPolygon, shortcut: 'p' },
];

interface PreviewState {
  draft: Draft;
  roof: RoofPreview | null;
  error: CommandError | null;
}

/**
 * The 2D plan (north up): grid, building footprints, roof faces with their edges, names.
 * Tools: edit (select a building, drag its vertices, add some from the « + » of an edge,
 * drag it whole, double-click a dimension to type a length), rectangle, polygon. While a
 * footprint is dragged its roof is previewed by Rust on every frame (refused corners in
 * red); a gesture ends with one command. Wheel zooms around the cursor; dragging the empty
 * plan (or with the middle / right button) pans.
 */
export function PlanView({ summary }: { summary: ProjectSummary }) {
  const { t: tr, i18n } = useTranslation();
  const host = useRef<HTMLDivElement>(null);
  const { w, h } = useElementSize(host);
  const colors = usePlanColors();
  const transform = usePlanStore((s) => s.transform);
  const setTransform = usePlanStore((s) => s.setTransform);
  const fitRequest = usePlanStore((s) => s.fitRequest);
  const requestFit = usePlanStore((s) => s.requestFit);
  const toolId = usePlanStore((s) => s.tool);
  const setTool = usePlanStore((s) => s.setTool);
  const selectedBuilding = useSelectionStore((s) => s.buildingId);
  const selectedFace = useSelectionStore((s) => s.faceId);

  const buildings = useMemo(() => planBuildings(summary), [summary]);
  const faces = useMemo(() => planFaces(summary), [summary]);
  const bounds = useMemo(() => planBounds(buildings, faces) ?? EMPTY_SITE, [buildings, faces]);

  // ---- tool, draft, preview ------------------------------------------------------------
  const tool = useRef<Tool>(createTool(toolId));
  const [view, setView] = useState<ToolView>(emptyView);
  useEffect(() => {
    tool.current = createTool(toolId);
    setView(tool.current.view());
  }, [toolId]);
  // A committed draft stays drawn until the project reloads (no flicker back).
  const [committed, setCommitted] = useState<{ draft: Draft; summary: ProjectSummary } | null>(
    null,
  );
  const draft = view.draft ?? (committed && committed.summary === summary ? committed.draft : null);

  const [preview, setPreview] = useState<PreviewState | null>(null);
  const previewer = useMemo(
    () =>
      new LatestOnly<{ draft: Draft; building: Building }, RoofPreview>(
        ({ building }) => unwrap(api.previewRoof(building)),
        (out, { draft: d }) => {
          setPreview(
            out instanceof Error
              ? { draft: d, roof: null, error: toCommandError(out) }
              : { draft: d, roof: out, error: null },
          );
        },
      ),
    [],
  );
  useEffect(() => {
    const d = view.draft;
    if (!d || d.buildingId === null || d.open) {
      previewer.cancel();
      return;
    }
    const building = draftBuilding(summary, d);
    if (building) previewer.request({ draft: d, building });
  }, [view.draft, summary, previewer]);
  // Buildings without roof specification get one before their first edit.
  useEffect(() => {
    if (selectedBuilding) void ensureSpec(summary, selectedBuilding);
  }, [summary, selectedBuilding]);

  const env = useCallback(
    (): ToolEnv => ({
      scale: usePlanStore.getState().transform?.scale ?? 1,
      buildings,
      selectedId: useSelectionStore.getState().buildingId,
    }),
    [buildings],
  );

  const commit = useCallback(
    (d: Draft | undefined) => {
      if (!d) return;
      setCommitted({ draft: d, summary });
      const name = nextBuildingName(summary, (n) => tr('building.defaultName', { n }));
      void commitDraft(summary, d, name).then((ok) => {
        if (!ok) setCommitted(null);
      });
    },
    [summary, tr],
  );

  // ---- fit -------------------------------------------------------------------------------
  const fitted = useRef<{ project: string; request: number } | null>(null);
  useEffect(() => {
    if (w === 0 || h === 0) return;
    const f = fitted.current;
    if (f && f.project === summary.projectId && f.request === fitRequest && transform) return;
    fitted.current = { project: summary.projectId, request: fitRequest };
    setTransform(fit(bounds, w, h));
  }, [w, h, bounds, summary.projectId, fitRequest, transform, setTransform]);

  // ---- dimensions (typed lengths) --------------------------------------------------------
  const closedDraft = draft && !draft.open ? draft : null;
  const dimensionPolygon: Point[] | null =
    closedDraft?.footprint ?? buildings.find((b) => b.id === selectedBuilding)?.footprint ?? null;
  const dimensionOwner = closedDraft ? closedDraft.buildingId : selectedBuilding;
  const fmt = useMemo(
    () =>
      new Intl.NumberFormat(i18n.language, { minimumFractionDigits: 2, maximumFractionDigits: 2 }),
    [i18n.language],
  );
  const dimensions: Dimension[] = useMemo(() => {
    const t = transform;
    if (!t || !dimensionPolygon || toolId !== 'select') return [];
    const n = dimensionPolygon.length;
    return dimensionPolygon.flatMap((a, i) => {
      const b = dimensionPolygon[(i + 1) % n] ?? a;
      const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      if (len * t.scale < 40) return [];
      // Outside the (counter-clockwise) polygon: to the right of the edge.
      const off = 14 / t.scale;
      const mid: Point = [
        (a[0] + b[0]) / 2 + ((b[1] - a[1]) / len) * off,
        (a[1] + b[1]) / 2 - ((b[0] - a[0]) / len) * off,
      ];
      const [x, y] = toScreen(t, mid);
      return [{ edge: i, x, y, text: `${fmt.format(len)} m`, value: fmt.format(len) }];
    });
  }, [transform, dimensionPolygon, toolId, fmt]);
  const [editing, setEditing] = useState<Dimension | null>(null);
  const applyLength = (edge: number, value: string) => {
    setEditing(null);
    const b = buildings.find((x) => x.id === dimensionOwner);
    const len = Number(value.replace(/\s/g, '').replace(',', '.'));
    if (!b || !Number.isFinite(len) || len <= 0.05) return;
    const poly = setEdgeLength(b.footprint, edge, len);
    if (!isSimple(poly)) return;
    const ccw = ensureCcw(poly);
    commit({
      buildingId: b.id,
      footprint: ccw.poly,
      changes: [ccw.change],
      delta: null,
      open: false,
    });
  };

  // ---- pointer and keys ------------------------------------------------------------------
  const dims = useRef(dimensions);
  useEffect(() => {
    dims.current = dimensions;
  }, [dimensions]);
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const local = (e: MouseEvent): [number, number] => {
      const r = el.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    };
    const world = (e: MouseEvent): Point | null => {
      const t = usePlanStore.getState().transform;
      return t ? toWorld(t, local(e)) : null;
    };
    const onUi = (e: Event) => e.target instanceof Element && e.target.closest('.plan-view__ui');
    /** The dimension label under the pointer (it opens on double-click). */
    const labelAt = (e: MouseEvent) => {
      const [x, y] = local(e);
      return dims.current.find((d) => Math.abs(d.x - x) < 36 && Math.abs(d.y - y) < 12);
    };
    let gesture:
      | { kind: 'tool' }
      | { kind: 'pan'; x: number; y: number; moved: boolean; button: number }
      | null = null;
    const sync = () => {
      setView(tool.current.view());
    };
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const cur = usePlanStore.getState().transform;
      if (cur) setTransform(zoomAt(cur, local(e), Math.exp(-e.deltaY * 0.0015)));
    };
    const onDown = (e: PointerEvent) => {
      // The overlays (tools, fit button, dimension input…) handle their own clicks.
      // A click on a dimension label is neutral (no deselection): its double-click edits it.
      if (onUi(e) || (e.button === 0 && labelAt(e))) return;
      el.setPointerCapture(e.pointerId);
      const p = world(e);
      if (e.button === 0 && p) {
        const r = tool.current.down({ world: p, alt: e.altKey }, env());
        sync();
        commit(r.commit);
        if (r.consumed) {
          gesture = { kind: 'tool' };
          return;
        }
      }
      gesture = { kind: 'pan', x: e.clientX, y: e.clientY, moved: false, button: e.button };
    };
    const onMove = (e: PointerEvent) => {
      if (gesture?.kind === 'pan') {
        const dx = e.clientX - gesture.x;
        const dy = e.clientY - gesture.y;
        if (!gesture.moved && Math.hypot(dx, dy) < CLICK_TOLERANCE_PX) return;
        gesture.moved = true;
        const cur = usePlanStore.getState().transform;
        if (cur) setTransform(pan(cur, dx, dy));
        gesture.x = e.clientX;
        gesture.y = e.clientY;
        return;
      }
      const p = world(e);
      if (p) {
        tool.current.move({ world: p, alt: e.altKey }, env());
        sync();
      }
    };
    const onUp = (e: PointerEvent) => {
      const g = gesture;
      gesture = null;
      const p = world(e);
      if (g?.kind === 'tool' && p) {
        const r = tool.current.up({ world: p, alt: e.altKey }, env());
        sync();
        commit(r.commit);
        return;
      }
      if (g?.kind !== 'pan' || g.moved || g.button !== 0 || !p) return;
      const hit = pick(faces, buildings, p);
      const sel = useSelectionStore.getState();
      if (!hit) sel.clear();
      else if (hit.faceId) sel.selectFace(hit.faceId, hit.buildingId);
      else sel.selectBuilding(hit.buildingId);
    };
    const onDouble = (e: MouseEvent) => {
      if (onUi(e)) return;
      const label = labelAt(e);
      if (label) {
        setEditing(label);
        return;
      }
      const p = world(e);
      if (!p) return;
      const r = tool.current.doubleClick({ world: p, alt: e.altKey }, env());
      sync();
      commit(r.commit);
    };
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('pointerdown', onDown);
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp);
    el.addEventListener('dblclick', onDouble);
    el.addEventListener('contextmenu', onContext);
    return () => {
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('pointerdown', onDown);
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('dblclick', onDouble);
      el.removeEventListener('contextmenu', onContext);
    };
  }, [faces, buildings, setTransform, env, commit]);

  // Keys go to the tool first (Enter, Backspace, Delete, Escape), then the tool shortcuts.
  useEffect(() => {
    usePlanStore.getState().setKeyHandler((key) => {
      const r = tool.current.key(key, env());
      setView(tool.current.view());
      commit(r.commit);
      if (r.consumed) return true;
      const shortcut = TOOLS.find((x) => x.shortcut === key.toLowerCase());
      if (shortcut) {
        setTool(shortcut.id);
        return true;
      }
      return false;
    });
    return () => {
      usePlanStore.getState().setKeyHandler(null);
    };
  }, [env, commit, setTool]);

  // ---- drawing -----------------------------------------------------------------------------
  const t = transform;
  const draftOf = (id: string) => (draft?.buildingId === id ? draft : null);
  const livePreview =
    preview && draft?.buildingId && preview.draft.buildingId === draft.buildingId ? preview : null;
  const shownFaces: { key: string; polygon: Point[]; edges: PlanEdge[]; selected: boolean }[] = [
    ...faces
      .filter((f) => !draftOf(f.buildingId))
      .map((f) => ({
        key: f.id,
        polygon: f.polygon,
        edges: f.edges,
        selected: f.id === selectedFace,
      })),
    ...(livePreview?.roof?.faces ?? []).map((f, i) => ({
      key: `preview-${i}`,
      polygon: f.polygon,
      edges: edgesOf(f.polygon, f.edgeKinds),
      selected: false,
    })),
  ];
  const draftError = livePreview?.error ?? null;
  const redEdges = closedDraft
    ? errorEdges(draftError, closedDraft.footprint.length)
    : new Set<number>();
  const handles = toolId === 'select' && !draft?.open ? (dimensionPolygon ?? []) : [];
  const message = view.error
    ? tr(view.error)
    : draftError
      ? tr(draftError.key, draftError.params)
      : null;
  const hintKey = toolId === 'select' && !selectedBuilding ? 'selectNone' : toolId;
  const cursorStyle =
    toolId !== 'select'
      ? 'crosshair'
      : view.hover === 'vertex'
        ? 'move'
        : view.hover === 'insert'
          ? 'copy'
          : view.hover === 'move'
            ? 'grab'
            : undefined;
  const cursorAt = t && view.cursor ? toScreen(t, view.cursor) : null;

  return (
    <div
      ref={host}
      className="plan-view"
      aria-label={tr('plan.label')}
      role="img"
      style={{ cursor: cursorStyle }}
    >
      {t && w > 0 && h > 0 && (
        <Stage width={w} height={h} listening={false}>
          <Layer listening={false}>
            <GridLines t={t} w={w} h={h} colors={colors} />
          </Layer>
          <Layer listening={false}>
            <Group x={t.x} y={t.y} scaleX={t.scale} scaleY={-t.scale}>
              {buildings
                .filter((b) => !draftOf(b.id))
                .map((b) => (
                  <Line
                    key={b.id}
                    points={flat(b.footprint)}
                    closed
                    fill={colors.footprint}
                    stroke={b.id === selectedBuilding ? colors.accent : colors.footprintStroke}
                    strokeWidth={b.id === selectedBuilding ? 2.5 : 1.5}
                    strokeScaleEnabled={false}
                  />
                ))}
              {draft && (
                <Line
                  points={flat(draft.footprint)}
                  closed={!draft.open}
                  fill={draft.open ? undefined : colors.footprint}
                  stroke={colors.accent}
                  strokeWidth={2}
                  dash={draft.open ? [6, 4] : undefined}
                  strokeScaleEnabled={false}
                />
              )}
              {shownFaces.map((f) => (
                <Line
                  key={f.key}
                  points={flat(f.polygon)}
                  closed
                  fill={f.selected ? colors.accent : colors.roof}
                  opacity={f.selected ? 0.35 : 1}
                  strokeEnabled={false}
                />
              ))}
              {shownFaces.flatMap((f) =>
                f.edges.map((edge, i) => (
                  <Line
                    key={`${f.key}-${i}`}
                    points={flat([edge.a, edge.b])}
                    stroke={colors.edge[edge.kind]}
                    strokeWidth={
                      edge.kind === 'ridge' || edge.kind === 'hip' || edge.kind === 'valley' ? 2 : 1
                    }
                    dash={edge.kind === 'verge' ? [6, 4] : undefined}
                    strokeScaleEnabled={false}
                  />
                )),
              )}
              {closedDraft &&
                [...redEdges].map((i) => (
                  <Line
                    key={`err-${i}`}
                    points={flat([
                      closedDraft.footprint[i] ?? [0, 0],
                      closedDraft.footprint[(i + 1) % closedDraft.footprint.length] ?? [0, 0],
                    ])}
                    stroke={colors.error}
                    strokeWidth={4}
                    strokeScaleEnabled={false}
                  />
                ))}
              {view.guides.map((g, i) => (
                <Line
                  key={`guide-${i}`}
                  points={flat([g.a, g.b])}
                  stroke={colors.accent}
                  strokeWidth={1}
                  dash={[4, 4]}
                  strokeScaleEnabled={false}
                />
              ))}
            </Group>
            {buildings
              .filter((b) => !draftOf(b.id))
              .map((b) => {
                const [x, y] = toScreen(t, centroid(b.footprint));
                return (
                  <Text
                    key={`label-${b.id}`}
                    x={x - 80}
                    y={y - 7}
                    width={160}
                    align="center"
                    text={b.name}
                    fontSize={12}
                    fontStyle={b.id === selectedBuilding ? 'bold' : 'normal'}
                    fill={colors.text}
                  />
                );
              })}
            {handles.map((p, i) => {
              const [x, y] = toScreen(t, p);
              const next = handles[(i + 1) % handles.length] ?? p;
              const [mx, my] = toScreen(t, [(p[0] + next[0]) / 2, (p[1] + next[1]) / 2]);
              return (
                <Group key={`h-${i}`}>
                  <Rect
                    x={x - HANDLE_PX / 2}
                    y={y - HANDLE_PX / 2}
                    width={HANDLE_PX}
                    height={HANDLE_PX}
                    fill={view.vertex === i ? colors.accent : colors.footprint}
                    stroke={colors.accent}
                    strokeWidth={1.5}
                  />
                  {!draft && (
                    <Circle
                      x={mx}
                      y={my}
                      radius={4}
                      fill={colors.footprint}
                      stroke={colors.accent}
                      strokeWidth={1}
                    />
                  )}
                </Group>
              );
            })}
            {cursorAt && <Circle x={cursorAt[0]} y={cursorAt[1]} radius={3} fill={colors.accent} />}
            {dimensions.map((d) => (
              <Text
                key={`dim-${d.edge}`}
                x={d.x - 40}
                y={d.y - 6}
                width={80}
                align="center"
                text={d.text}
                fontSize={11}
                fill={redEdges.has(d.edge) ? colors.error : colors.text}
              />
            ))}
          </Layer>
        </Stage>
      )}
      <div
        className="plan-view__ui plan-view__tools"
        role="toolbar"
        aria-label={tr('plan.tools.label')}
      >
        {TOOLS.map(({ id, icon: Icon, shortcut }) => (
          <IconButton
            key={id}
            label={`${tr(`plan.tools.${id}`)} (${shortcut.toUpperCase()})`}
            icon={<Icon />}
            active={toolId === id}
            onClick={() => {
              setTool(id);
            }}
          />
        ))}
      </div>
      <div className="plan-view__ui plan-view__overlay">
        <span className="plan-view__scale">
          {tr('plan.grid', { step: t ? gridStep(t.scale) : 1 })}
        </span>
        <IconButton label={tr('plan.fit')} icon={<IconFit />} onClick={requestFit} />
      </div>
      <p
        className={message ? 'plan-view__hint plan-view__hint--error' : 'plan-view__hint'}
        role="status"
      >
        {message ?? tr(`plan.hints.${hintKey}`)}
      </p>
      {editing && (
        <input
          className="plan-view__ui plan-view__dimension"
          style={{ left: editing.x - 40, top: editing.y - 12 }}
          aria-label={tr('plan.edgeLength')}
          autoFocus
          defaultValue={editing.value}
          onKeyDown={(e) => {
            if (e.key === 'Enter') applyLength(editing.edge, e.currentTarget.value);
            if (e.key === 'Escape') setEditing(null);
          }}
          onBlur={() => {
            setEditing(null);
          }}
        />
      )}
      {summary.buildings.length === 0 && !draft && (
        <p className="plan-view__empty">{tr('plan.empty')}</p>
      )}
    </div>
  );
}
