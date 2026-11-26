import { useEffect } from 'react';
import { useThree } from '@react-three/fiber';
import { Matrix4, Raycaster, Vector2 } from 'three';
import { useSelectionStore } from '../store/selection';
import { useShadingStore } from '../store/shading';
import { useViewStore } from '../store/view';
import type { Editor } from './editor';
import { hitRoofs, planeUV, type V3 } from './roofHit';

/** Pointer travel (px) below which a press/release is a click. */
const CLICK_TOLERANCE_PX = 4;

type Gesture =
  | { kind: 'click'; x: number; y: number }
  | { kind: 'box'; x: number; y: number; additive: boolean }
  | { kind: 'drag'; faceIndex: number };

/**
 * Pointer handling on the canvas. Listens in the capture phase so it can claim a gesture
 * (box select, drag move) before OrbitControls sees it; other gestures orbit the camera.
 */
export function Interaction({ editor }: { editor: Editor }) {
  const gl = useThree((s) => s.gl);
  const getThree = useThree((s) => s.get);

  useEffect(() => {
    const el = gl.domElement;
    const raycaster = new Raycaster();
    const ndc = new Vector2();
    const overlay = document.createElement('div');
    overlay.className = 'box-select';
    overlay.hidden = true;
    el.parentElement?.appendChild(overlay);
    let gesture: Gesture | null = null;

    const toNdc = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      return ndc.set(
        ((e.clientX - r.left) / r.width) * 2 - 1,
        -((e.clientY - r.top) / r.height) * 2 + 1,
      );
    };
    const ray = (e: PointerEvent): { origin: V3; dir: V3 } => {
      raycaster.setFromCamera(toNdc(e), getThree().camera);
      const { origin, direction } = raycaster.ray;
      return {
        origin: [origin.x, origin.y, origin.z],
        dir: [direction.x, direction.y, direction.z],
      };
    };
    const claim = (e: PointerEvent) => {
      e.stopImmediatePropagation(); // keep OrbitControls out of this gesture
      el.setPointerCapture(e.pointerId);
    };

    const onDown = (e: PointerEvent) => {
      const tool = useViewStore.getState().tool;
      if ((e.button === 2 && e.shiftKey) || (tool === 'box' && e.button === 0)) {
        gesture = { kind: 'box', x: e.clientX, y: e.clientY, additive: e.shiftKey };
        claim(e);
        return;
      }
      if (e.button !== 0) return;
      if (tool === 'move') {
        const { origin, dir } = ray(e);
        const hit = hitRoofs(editor.summary.roofFaces, origin, dir);
        const id = hit && editor.pick(hit.faceIndex, hit.u, hit.v);
        if (hit && id !== null && editor.selection.has(id) && editor.beginDrag(hit.u, hit.v)) {
          gesture = { kind: 'drag', faceIndex: hit.faceIndex };
          claim(e);
          return;
        }
      }
      gesture = { kind: 'click', x: e.clientX, y: e.clientY };
    };

    // Shading mode: the module under the pointer (once per frame).
    let hoverFrame = 0;
    const hover = (e: PointerEvent) => {
      if (hoverFrame) return;
      const { clientX, clientY } = e;
      hoverFrame = requestAnimationFrame(() => {
        hoverFrame = 0;
        const shading = useShadingStore.getState();
        if (useViewStore.getState().colorMode !== 'shading') {
          if (shading.hover) shading.setHover(null);
          return;
        }
        const r = el.getBoundingClientRect();
        raycaster.setFromCamera(
          ndc.set(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1),
          getThree().camera,
        );
        const { origin: o, direction: d } = raycaster.ray;
        const hit = hitRoofs(editor.summary.roofFaces, [o.x, o.y, o.z], [d.x, d.y, d.z]);
        const id = hit ? editor.pick(hit.faceIndex, hit.u, hit.v) : null;
        shading.setHover(id === null ? null : { id, x: clientX - r.left, y: clientY - r.top });
      });
    };
    const onLeave = () => {
      cancelAnimationFrame(hoverFrame);
      hoverFrame = 0;
      if (useShadingStore.getState().hover) useShadingStore.getState().setHover(null);
    };

    const onMove = (e: PointerEvent) => {
      if (!gesture) {
        hover(e);
        return;
      }
      if (gesture.kind === 'box') {
        const r = el.getBoundingClientRect();
        overlay.hidden = false;
        overlay.style.left = `${Math.min(gesture.x, e.clientX) - r.left}px`;
        overlay.style.top = `${Math.min(gesture.y, e.clientY) - r.top}px`;
        overlay.style.width = `${Math.abs(e.clientX - gesture.x)}px`;
        overlay.style.height = `${Math.abs(e.clientY - gesture.y)}px`;
      } else if (gesture.kind === 'drag') {
        const face = editor.summary.roofFaces[gesture.faceIndex];
        const { origin, dir } = ray(e);
        const hit = face && planeUV(face, origin, dir);
        if (hit) editor.dragTo(hit.u, hit.v);
      }
    };

    const onUp = (e: PointerEvent) => {
      const g = gesture;
      gesture = null;
      if (!g) return;
      if (g.kind === 'drag') {
        void editor.endDrag();
        return;
      }
      if (g.kind === 'box') {
        overlay.hidden = true;
        const r = el.getBoundingClientRect();
        const nx = (x: number) => ((x - r.left) / r.width) * 2 - 1;
        const ny = (y: number) => -((y - r.top) / r.height) * 2 + 1;
        const { camera } = getThree();
        camera.updateMatrixWorld();
        const vp = new Matrix4().multiplyMatrices(
          camera.projectionMatrix,
          camera.matrixWorldInverse,
        );
        void editor.boxSelect(
          vp.elements,
          { x0: nx(g.x), y0: ny(g.y), x1: nx(e.clientX), y1: ny(e.clientY) },
          g.additive,
        );
        return;
      }
      if (Math.hypot(e.clientX - g.x, e.clientY - g.y) > CLICK_TOLERANCE_PX) return; // orbit
      const { origin, dir } = ray(e);
      const hit = hitRoofs(editor.summary.roofFaces, origin, dir);
      // The clicked roof face (and its building) becomes the shared selection.
      const face = hit ? editor.summary.roofFaces[hit.faceIndex]?.face : undefined;
      if (face) useSelectionStore.getState().selectFace(face.id, face.buildingId);
      else if (!e.shiftKey) useSelectionStore.getState().clear();
      const id = hit ? editor.pick(hit.faceIndex, hit.u, hit.v) : null;
      if (id === null) {
        if (!e.shiftKey) editor.clearSelection();
      } else if (e.shiftKey) {
        editor.toggle(id);
      } else {
        editor.select([id]);
      }
    };

    const onCancel = () => {
      if (gesture?.kind === 'drag') editor.cancelDrag();
      overlay.hidden = true;
      gesture = null;
    };
    const onContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };

    el.addEventListener('pointerdown', onDown, { capture: true });
    el.addEventListener('pointermove', onMove);
    el.addEventListener('pointerup', onUp);
    el.addEventListener('pointercancel', onCancel);
    el.addEventListener('contextmenu', onContextMenu);
    el.addEventListener('pointerleave', onLeave);
    return () => {
      el.removeEventListener('pointerleave', onLeave);
      onLeave();
      el.removeEventListener('pointerdown', onDown, { capture: true });
      el.removeEventListener('pointermove', onMove);
      el.removeEventListener('pointerup', onUp);
      el.removeEventListener('pointercancel', onCancel);
      el.removeEventListener('contextmenu', onContextMenu);
      overlay.remove();
    };
  }, [gl, getThree, editor]);

  return null;
}
