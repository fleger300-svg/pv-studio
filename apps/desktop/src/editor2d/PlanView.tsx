import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Group, Layer, Line, Stage, Text } from 'react-konva';
import { IconButton } from '@pv/ui';
import type { ProjectSummary, RoofEdgeKind } from '../bindings';
import { IconFit } from '../shell/icons';
import { usePlanStore } from '../store/plan';
import { useSelectionStore } from '../store/selection';
import { useUiStore } from '../store/ui';
import {
  centroid,
  contains,
  planBounds,
  planBuildings,
  planFaces,
  type PlanBuilding,
  type PlanFace,
} from './planGeometry';
import {
  fit,
  gridStep,
  pan,
  toScreen,
  toWorld,
  visibleBounds,
  zoomAt,
  type ViewTransform,
} from './viewTransform';

/** Pointer travel (px) below which a press/release is a click, not a pan. */
const CLICK_TOLERANCE_PX = 4;
/** An empty site is shown as 40 m around the origin. */
const EMPTY_SITE = { minE: -20, minN: -20, maxE: 20, maxN: 20 };

interface PlanColors {
  grid: string;
  gridMajor: string;
  footprint: string;
  footprintStroke: string;
  roof: string;
  text: string;
  accent: string;
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

/**
 * The 2D plan (north up): grid, building footprints, roof faces with their edges (ridges,
 * hips, valleys), names. Wheel zooms around the cursor, dragging pans, a click selects a
 * roof face or a building (shared with the 3D view and the panels). Read-only for now.
 */
export function PlanView({ summary }: { summary: ProjectSummary }) {
  const { t: tr } = useTranslation();
  const host = useRef<HTMLDivElement>(null);
  const { w, h } = useElementSize(host);
  const colors = usePlanColors();
  const transform = usePlanStore((s) => s.transform);
  const setTransform = usePlanStore((s) => s.setTransform);
  const fitRequest = usePlanStore((s) => s.fitRequest);
  const requestFit = usePlanStore((s) => s.requestFit);
  const selectedBuilding = useSelectionStore((s) => s.buildingId);
  const selectedFace = useSelectionStore((s) => s.faceId);

  const buildings = useMemo(() => planBuildings(summary), [summary]);
  const faces = useMemo(() => planFaces(summary), [summary]);
  const bounds = useMemo(() => planBounds(buildings, faces) ?? EMPTY_SITE, [buildings, faces]);

  // Fit once per project (and on request); later edits keep the user's view.
  const fitted = useRef<{ project: string; request: number } | null>(null);
  useEffect(() => {
    if (w === 0 || h === 0) return;
    const f = fitted.current;
    if (f && f.project === summary.projectId && f.request === fitRequest && transform) return;
    fitted.current = { project: summary.projectId, request: fitRequest };
    setTransform(fit(bounds, w, h));
  }, [w, h, bounds, summary.projectId, fitRequest, transform, setTransform]);

  // Wheel zoom and pointer pan / click, on the host element (native listeners: the wheel
  // must not scroll the page).
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const local = (e: MouseEvent): [number, number] => {
      const r = el.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    };
    let drag: { x: number; y: number; moved: boolean; button: number } | null = null;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const cur = usePlanStore.getState().transform;
      if (cur) setTransform(zoomAt(cur, local(e), Math.exp(-e.deltaY * 0.0015)));
    };
    const onDown = (e: PointerEvent) => {
      // The overlay (fit button…) handles its own clicks: capturing would swallow them.
      if (e.target instanceof Element && e.target.closest('.plan-view__overlay')) return;
      drag = { x: e.clientX, y: e.clientY, moved: false, button: e.button };
      el.setPointerCapture(e.pointerId);
    };
    const onMove = (e: PointerEvent) => {
      if (!drag) return;
      const dx = e.clientX - drag.x;
      const dy = e.clientY - drag.y;
      if (!drag.moved && Math.hypot(dx, dy) < CLICK_TOLERANCE_PX) return;
      drag.moved = true;
      const cur = usePlanStore.getState().transform;
      if (cur) setTransform(pan(cur, dx, dy));
      drag.x = e.clientX;
      drag.y = e.clientY;
    };
    const onUp = (e: PointerEvent) => {
      const d = drag;
      drag = null;
      const cur = usePlanStore.getState().transform;
      if (!d || d.moved || d.button !== 0 || !cur) return;
      const hit = pick(faces, buildings, toWorld(cur, local(e)));
      const sel = useSelectionStore.getState();
      if (!hit) sel.clear();
      else if (hit.faceId) sel.selectFace(hit.faceId, hit.buildingId);
      else sel.selectBuilding(hit.buildingId);
    };
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('pointerdown', onDown);
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp);
    el.addEventListener('contextmenu', onContext);
    return () => {
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('pointerdown', onDown);
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('contextmenu', onContext);
    };
  }, [faces, buildings, setTransform]);

  const t = transform;
  return (
    <div ref={host} className="plan-view" aria-label={tr('plan.label')} role="img">
      {t && w > 0 && h > 0 && (
        <Stage width={w} height={h} listening={false}>
          <Layer listening={false}>
            <GridLines t={t} w={w} h={h} colors={colors} />
          </Layer>
          <Layer listening={false}>
            <Group x={t.x} y={t.y} scaleX={t.scale} scaleY={-t.scale}>
              {buildings.map((b) => (
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
              {faces.map((f) => (
                <Line
                  key={f.id}
                  points={flat(f.polygon)}
                  closed
                  fill={f.id === selectedFace ? colors.accent : colors.roof}
                  opacity={f.id === selectedFace ? 0.35 : 1}
                  strokeEnabled={false}
                />
              ))}
              {faces.flatMap((f) =>
                f.edges.map((edge, i) => (
                  <Line
                    key={`${f.id}-${i}`}
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
            </Group>
            {buildings.map((b) => {
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
          </Layer>
        </Stage>
      )}
      <div className="plan-view__overlay">
        <span className="plan-view__scale">
          {tr('plan.grid', { step: t ? gridStep(t.scale) : 1 })}
        </span>
        <IconButton label={tr('plan.fit')} icon={<IconFit />} onClick={requestFit} />
      </div>
      {summary.buildings.length === 0 && <p className="plan-view__empty">{tr('plan.empty')}</p>}
    </div>
  );
}
