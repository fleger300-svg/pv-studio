import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, type ComponentRef } from 'react';
import { Canvas, useThree } from '@react-three/fiber';
import { OrbitControls } from '@react-three/drei';
import { Object3D, Vector3, type DirectionalLight } from 'three';
import { tokens } from '@pv/ui';
import type { ProjectSummary } from '../bindings';
import type { ModuleData } from './moduleBuffer';
import { Buildings } from './Buildings';
import { sceneBounds } from './frames';
import { PanelMeshes } from './PanelMeshes';
import { GridIndex } from './gridIndex';
import { Editor } from './editor';
import { Interaction } from './Interaction';
import { ipcCallCount } from '../ipc';
import { FrameProbe } from '../perf/FrameProbe';
import { usePerfStore } from '../perf/perfStore';
import { usePlanStore } from '../store/plan';
import { useProjectStore } from '../store/project';
import { useSelectionStore } from '../store/selection';
import { useViewStore } from '../store/view';
import { useWiringStore } from '../store/wiring';
import { useWorkflowStore } from '../store/workflow';
import { useUiStore } from '../store/ui';

// World frame is ENU with z up.
Object3D.DEFAULT_UP.set(0, 0, 1);

/** Sun direction (towards the sun): azimuth 200° (SSW), elevation 40°. */
const SUN_DIR = (() => {
  const az = (200 * Math.PI) / 180;
  const el = (40 * Math.PI) / 180;
  return new Vector3(Math.sin(az) * Math.cos(el), Math.cos(az) * Math.cos(el), Math.sin(el));
})();

interface SceneProps {
  summary: ProjectSummary;
  modules: ModuleData;
}

function Scene({ summary, modules }: SceneProps) {
  const getThree = useThree((s) => s.get);
  const invalidate = useThree((s) => s.invalidate);
  const controls = useRef<ComponentRef<typeof OrbitControls>>(null);
  const sun = useRef<DirectionalLight>(null);
  const colorMode = useViewStore((s) => s.colorMode);
  const ground = tokens[useUiStore((s) => s.theme)].ground;

  const panels = useMemo(() => new PanelMeshes(summary, modules), [summary, modules]);
  // Static shadows: light and geometry do not move with the camera, so the shadow map is
  // rendered only when the model changes (load, after a command), not on every frame.
  const redraw = useCallback(
    (opts?: { shadows?: boolean }) => {
      if (opts?.shadows) getThree().gl.shadowMap.needsUpdate = true;
      invalidate();
    },
    [getThree, invalidate],
  );
  useLayoutEffect(() => {
    const { gl } = getThree();
    gl.shadowMap.autoUpdate = false;
    gl.shadowMap.needsUpdate = true;
  }, [getThree, panels]);
  const editor = useMemo(
    () =>
      new Editor(
        summary,
        panels,
        new GridIndex(summary, modules),
        redraw,
        usePerfStore.getState().recordCommand,
      ),
    [summary, modules, panels, redraw],
  );
  useEffect(() => {
    void editor.attach();
    useViewStore.setState({ editor, three: getThree, selectionCount: editor.selection.size });
    // Dev-only automation handle (stripped from production builds).
    if (import.meta.env.DEV) {
      window.__pv = {
        editor,
        three: getThree,
        ipcCallCount,
        project: useProjectStore,
        selection: useSelectionStore,
        workflow: useWorkflowStore,
        plan: usePlanStore,
      };
    }
    return () => {
      useViewStore.setState({ editor: null, three: null, selectionCount: 0 });
      editor.detach();
    };
  }, [editor, getThree]);
  // GPU resources are freed when the project changes (three.js re-uploads on demand, so a
  // StrictMode remount is harmless).
  useEffect(
    () => () => {
      panels.dispose();
    },
    [panels],
  );

  const bounds = useMemo(() => sceneBounds(summary, modules), [summary, modules]);
  const center = useMemo(() => bounds.getCenter(new Vector3()), [bounds]);
  const radius = useMemo(() => bounds.getSize(new Vector3()).length() / 2, [bounds]);

  // Frame the project once per load, when the first building appears on an empty site,
  // and on request (toolbar): from the south-east, looking down ~35°. Edits keep the
  // user's camera.
  const frameRequest = useViewStore((s) => s.frameRequest);
  const framed = useRef<{ project: string; empty: boolean; request: number } | null>(null);
  const empty = summary.buildings.length === 0 && modules.count === 0;
  useLayoutEffect(() => {
    const f = framed.current;
    if (
      f &&
      f.project === summary.projectId &&
      f.request === frameRequest &&
      !(f.empty && !empty)
    ) {
      return;
    }
    framed.current = { project: summary.projectId, empty, request: frameRequest };
    // Three.js objects are mutable by design; read them from the store inside the effect.
    const { camera } = getThree();
    const dir = new Vector3(0.35, -0.8, 0.6).normalize();
    camera.position.copy(center).addScaledVector(dir, radius * 2.4);
    camera.near = Math.max(0.1, radius / 500);
    camera.far = radius * 20;
    camera.updateProjectionMatrix();
    controls.current?.target.copy(center);
    controls.current?.update();
    invalidate();
  }, [getThree, center, radius, invalidate, summary.projectId, empty, frameRequest]);

  // Sun with a shadow camera fitted tightly around the project.
  useLayoutEffect(() => {
    const light = sun.current;
    if (!light) return;
    light.position.copy(center).addScaledVector(SUN_DIR, radius * 2);
    light.target.position.copy(center);
    light.target.updateMatrixWorld();
    const cam = light.shadow.camera;
    cam.left = -radius;
    cam.right = radius;
    cam.top = radius;
    cam.bottom = -radius;
    cam.near = radius * 0.5;
    cam.far = radius * 3.5;
    cam.updateProjectionMatrix();
    // Texel size grows with the project; scale the normal bias with it to avoid acne on
    // the panels while keeping contact shadows.
    const texel = (2 * radius) / light.shadow.mapSize.width;
    light.shadow.bias = -0.0005;
    light.shadow.normalBias = texel * 1.5;
    light.shadow.needsUpdate = true;
    redraw({ shadows: true });
  }, [center, radius, redraw]);

  useEffect(() => {
    editor.setColorMode(colorMode);
  }, [editor, colorMode]);

  const strings = useWiringStore((s) => s.stringOf);
  useEffect(() => {
    editor.setStrings(strings);
  }, [editor, strings]);

  return (
    <>
      <hemisphereLight args={['#dfe9f5', '#7a7466', 0.9]} />
      <directionalLight
        ref={sun}
        intensity={2.2}
        castShadow
        shadow-mapSize-width={2048}
        shadow-mapSize-height={2048}
      />
      <mesh position={[center.x, center.y, 0]} receiveShadow>
        <planeGeometry args={[radius * 8, radius * 8]} />
        <meshStandardMaterial color={ground} roughness={1} />
      </mesh>
      <Buildings summary={summary} />
      <primitive object={panels.group} />
      <Interaction editor={editor} />
      <FrameProbe key={summary.projectId} />
      <OrbitControls
        ref={controls}
        makeDefault
        enableDamping
        dampingFactor={0.12}
        maxPolarAngle={Math.PI * 0.49}
      />
    </>
  );
}

export function Viewport({ summary, modules }: SceneProps) {
  const theme = useUiStore((s) => s.theme);
  const background = tokens[theme].viewportBg;
  return (
    <Canvas
      frameloop="demand"
      shadows="soft"
      dpr={[1, 2]}
      camera={{ fov: 45, position: [0, -50, 40] }}
      gl={{ antialias: true, powerPreference: 'high-performance' }}
    >
      <color attach="background" args={[background]} />
      <Scene summary={summary} modules={modules} />
    </Canvas>
  );
}
