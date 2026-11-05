import { Vector3 } from 'three';
import type { RootState } from '@react-three/fiber';
import type { StressPreset } from '../bindings';
import { api, BackendError, ipcCallCount, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useViewStore } from '../store/view';
import type { CommandTiming, Editor } from '../viewport3d/editor';
import { FLAG_REMOVED } from '../viewport3d/moduleBuffer';
import { frameRecorder } from './frames';
import { usePerfStore, type BenchStep } from './perfStore';
import { summarizeFrames, type FrameSummary } from './stats';

const ORBIT_MS = 10_000;
const DRAG_MS = 2_000;
const DRAG_MODULES = 100;
/** Drag displacement along roof u (m): small enough to stay inside every preset's roof. */
const DRAG_DU = 0.25;

/** Acceptance budgets (README / spec §12). */
export const BUDGETS: Record<
  StressPreset,
  { orbitFps: number; dragFps: number; moveRoundTripMs: number; openMs: number; memoryMB: number }
> = {
  Residential1k: { orbitFps: 60, dragFps: 60, moveRoundTripMs: 100, openMs: 1000, memoryMB: 400 },
  Industrial10k: { orbitFps: 30, dragFps: 60, moveRoundTripMs: 500, openMs: 3000, memoryMB: 1024 },
};
/** FPS budgets are met when within 2 % (vsync quantisation on a display of that rate). */
const FPS_TOLERANCE = 0.98;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitFor<T>(probe: () => T | null | undefined, timeoutMs = 30_000): Promise<T> {
  const deadline = performance.now() + timeoutMs;
  for (;;) {
    const v = probe();
    if (v !== null && v !== undefined) return v;
    if (performance.now() > deadline) throw new Error('benchmark: timed out waiting');
    await sleep(20);
  }
}

function gpuRenderer(state: RootState): string | null {
  const gl = state.gl.getContext();
  const ext = gl.getExtension('WEBGL_debug_renderer_info');
  if (!ext) return null;
  const value: unknown = gl.getParameter(ext.UNMASKED_RENDERER_WEBGL);
  return typeof value === 'string' ? value : null;
}

/** `performance.memory` exists in Chromium (WebView2) only. */
export function jsHeapMB(): number | null {
  const memory = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory;
  return memory ? memory.usedJSHeapSize / (1024 * 1024) : null;
}

/** Orbits the camera around the controls' target at constant angular speed. */
async function orbit(three: RootState, durationMs: number): Promise<FrameSummary> {
  const controls = three.controls as unknown as {
    target: Vector3;
    enabled: boolean;
    update: () => void;
  } | null;
  const target = controls?.target.clone() ?? new Vector3();
  const offset = three.camera.position.clone().sub(target);
  const radius = Math.hypot(offset.x, offset.y);
  const start = Math.atan2(offset.y, offset.x);
  const omega = (2 * Math.PI) / durationMs; // one full turn
  const stamps: number[] = [];
  if (controls) controls.enabled = false;
  let t0: number | null = null;
  await frameRecorder.run((now) => {
    t0 ??= now;
    const angle = start + omega * (now - t0);
    three.camera.position.set(
      target.x + radius * Math.cos(angle),
      target.y + radius * Math.sin(angle),
      target.z + offset.z,
    );
    three.camera.lookAt(target);
    stamps.push(now);
    return now - t0 < durationMs;
  }, three.invalidate);
  if (controls) {
    controls.enabled = true;
    controls.update();
  }
  return summarizeFrames(stamps);
}

/** First `n` active modules of the first array (deterministic selection). */
function pickModules(editor: Editor, n: number): number[] {
  const first = editor.summary.arrays[0];
  const ids: number[] = [];
  if (!first) return ids;
  const data = editor.panels.data;
  for (let id = first.firstModuleId; id < first.firstModuleId + first.moduleCount; id++) {
    if (((data.flags[id] ?? 0) & FLAG_REMOVED) === 0) ids.push(id);
    if (ids.length === n) break;
  }
  return ids;
}

/** Drags the selection for `durationMs` (local preview only) and counts IPC calls meanwhile. */
async function drag(
  editor: Editor,
  three: RootState,
  ids: number[],
): Promise<FrameSummary & { ipcCalls: number }> {
  editor.select(ids);
  const anchor = ids[0] ?? 0;
  const u = editor.grid.uv[anchor * 2] ?? 0;
  const v = editor.grid.uv[anchor * 2 + 1] ?? 0;
  if (!editor.beginDrag(u, v)) throw new Error('benchmark: drag could not start');
  const stamps: number[] = [];
  const ipcBefore = ipcCallCount();
  let t0: number | null = null;
  await frameRecorder.run((now) => {
    t0 ??= now;
    const k = Math.min(1, (now - t0) / DRAG_MS);
    editor.dragTo(u + DRAG_DU * k, v);
    stamps.push(now);
    return now - t0 < DRAG_MS;
  }, three.invalidate);
  return { ...summarizeFrames(stamps), ipcCalls: ipcCallCount() - ipcBefore };
}

/**
 * Scripted, reproducible benchmark on the current preset. Returns the report file path
 * (written by Rust: `testdata/` in dev builds, app data dir in release builds).
 */
export async function runBenchmark(): Promise<string> {
  const perf = usePerfStore.getState();
  const step = (s: BenchStep) => {
    perf.setBench({ state: 'running', step: s });
  };
  const preset = useProjectStore.getState().preset ?? 'Residential1k';

  // 1. Open time (regenerate the file, then a timed open → first rendered frame).
  step('open');
  perf.resetOpenMs();
  await useProjectStore.getState().loadPreset(preset);
  const loadError = useProjectStore.getState().error;
  if (loadError) throw new BackendError(loadError);
  const openMs = await waitFor(() => usePerfStore.getState().openMs);
  const summary = useProjectStore.getState().summary;
  const editor = await waitFor(() => {
    const e = useViewStore.getState().editor;
    return e && e.summary === summary ? e : null;
  });
  const three = (await waitFor(() => useViewStore.getState().three))();
  await sleep(500); // shader warm-up, shadow map

  const timings: CommandTiming[] = [];
  const previous = editor.onCommand;
  editor.onCommand = (t) => {
    timings.push(t);
    previous?.(t);
  };
  try {
    // 2. Orbit.
    step('orbit');
    const orbitStats = await orbit(three, ORBIT_MS);
    const renderer = { drawCalls: frameRecorder.calls, triangles: frameRecorder.triangles };

    // 3. Drag 100 modules for 2 s (GPU only).
    step('drag');
    const ids = pickModules(editor, DRAG_MODULES);
    const dragStats = await drag(editor, three, ids);

    // 4. One MoveModules on release.
    step('move');
    await editor.endDrag();
    await sleep(300); // model:changed resync
    // 5. Undo, redo.
    step('undo');
    await editor.undo();
    await sleep(300);
    step('redo');
    await editor.redo();
    await sleep(300);
    editor.clearSelection();

    step('report');
    const find = (kind: CommandTiming['kind']) => timings.find((t) => t.kind === kind) ?? null;
    const move = find('apply');
    const platform = await api.platformInfo();
    const budget = BUDGETS[preset];
    const heap = jsHeapMB();
    const report = {
      schema: 'pv-studio-bench/1',
      createdAt: new Date().toISOString(),
      preset,
      moduleCount: editor.panels.instanceCount,
      instancedMeshes: editor.panels.meshes.length,
      environment: {
        os: platform.os,
        arch: platform.arch,
        rustDebugBuild: platform.debugBuild,
        frontendDevBuild: import.meta.env.DEV,
        userAgent: navigator.userAgent,
        gpuRenderer: gpuRenderer(three),
        screen: { width: screen.width, height: screen.height },
        devicePixelRatio: window.devicePixelRatio,
        canvasPx: { width: three.gl.domElement.width, height: three.gl.domElement.height },
        estimatedRefreshHz: orbitStats.medianFrameMs > 0 ? 1000 / orbitStats.medianFrameMs : null,
      },
      results: {
        openToFirstFrameMs: openMs,
        orbit: orbitStats,
        renderer,
        drag: { modules: ids.length, ...dragStats },
        moveModules: move && { roundTripMs: move.roundTripMs, rustUs: move.rustUs },
        undo: find('undo'),
        redo: find('redo'),
        jsHeapMB: heap,
      },
      budgets: {
        ...budget,
        pass: {
          orbitFps: orbitStats.avgFps >= budget.orbitFps * FPS_TOLERANCE,
          dragFps: dragStats.avgFps >= budget.dragFps * FPS_TOLERANCE,
          dragZeroIpc: dragStats.ipcCalls === 0,
          drawCallsOnePerArray: editor.panels.meshes.length === editor.summary.arrays.length,
          moveRoundTrip: move !== null && move.roundTripMs < budget.moveRoundTripMs,
          openTime: openMs < budget.openMs,
          jsHeap: heap === null ? null : heap < budget.memoryMB,
        },
      },
    };
    return await unwrap(api.writeBenchReport(preset, JSON.stringify(report)));
  } finally {
    editor.onCommand = previous;
  }
}
