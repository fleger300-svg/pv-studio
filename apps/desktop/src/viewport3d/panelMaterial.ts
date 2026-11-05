import {
  BoxGeometry,
  BufferGeometry,
  Color,
  Float32BufferAttribute,
  MeshStandardMaterial,
  PlaneGeometry,
} from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';

/** Width of the aluminium frame visible around the cell area (m). */
const FRAME_WIDTH_M = 0.035;
/** Light frame colour (constant, not affected by selection or string mode). */
const FRAME_COLOR = new Color('#d5dae1');

function withFrameMask(geometry: BufferGeometry, mask: number): BufferGeometry {
  const count = geometry.getAttribute('position').count;
  geometry.setAttribute(
    'frameMask',
    new Float32BufferAttribute(new Float32Array(count).fill(mask), 1),
  );
  return geometry;
}

/** A flat quad in the top plane, as non-indexed geometry. */
function topQuad(w: number, h: number, x: number, y: number, z: number, mask: number) {
  const quad = new PlaneGeometry(w, h).toNonIndexed();
  quad.translate(x, y, z);
  return withFrameMask(quad, mask);
}

/**
 * Module geometry in its local frame (x across, y along slope, z normal): the frame box
 * without its top face, a frame ring on top (`frameMask = 1`) and the cell area inside it
 * (`frameMask = 0`). Ring and cells share one plane without overlapping, so there is no
 * z-fighting at any distance. 20 triangles per module.
 */
export function createModuleGeometry(sx: number, sy: number, sz: number): BufferGeometry {
  const box = new BoxGeometry(sx, sy, sz).toNonIndexed();
  // Drop the +z face (BoxGeometry face order: +x, −x, +y, −y, +z, −z; 6 vertices each).
  const keep = (attr: string) => {
    const a = box.getAttribute(attr);
    const out = new Float32Array((a.count - 6) * a.itemSize);
    out.set((a.array as Float32Array).subarray(0, 24 * a.itemSize));
    out.set((a.array as Float32Array).subarray(30 * a.itemSize), 24 * a.itemSize);
    return new Float32BufferAttribute(out, a.itemSize);
  };
  const sides = new BufferGeometry();
  for (const attr of ['position', 'normal', 'uv']) sides.setAttribute(attr, keep(attr));
  withFrameMask(sides, 1);
  box.dispose();

  const f = FRAME_WIDTH_M;
  const z = sz / 2;
  const innerW = sx - 2 * f;
  const innerH = sy - 2 * f;
  const parts = [
    sides,
    topQuad(sx, f, 0, sy / 2 - f / 2, z, 1),
    topQuad(sx, f, 0, -sy / 2 + f / 2, z, 1),
    topQuad(f, innerH, sx / 2 - f / 2, 0, z, 1),
    topQuad(f, innerH, -sx / 2 + f / 2, 0, z, 1),
    topQuad(innerW, innerH, 0, 0, z, 0),
  ];
  const merged = mergeGeometries(parts);
  for (const p of parts) p.dispose();
  return merged;
}

/**
 * One material for all panels. The per-instance colour (`instanceColor`) is used *as is*
 * for the cell area — so a selected module is exactly `#FF6727` — while the frame keeps a
 * fixed light grey. Implemented by mixing after three.js' `color_fragment` chunk.
 */
export function createPanelMaterial(): MeshStandardMaterial {
  const material = new MeshStandardMaterial({ color: 0xffffff, roughness: 0.35, metalness: 0.15 });
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uFrameColor = { value: FRAME_COLOR };
    shader.vertexShader = shader.vertexShader
      .replace(
        '#include <common>',
        '#include <common>\nattribute float frameMask;\nvarying float vFrame;',
      )
      .replace('#include <begin_vertex>', '#include <begin_vertex>\nvFrame = frameMask;');
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        '#include <common>\nuniform vec3 uFrameColor;\nvarying float vFrame;',
      )
      .replace(
        '#include <color_fragment>',
        '#include <color_fragment>\ndiffuseColor.rgb = mix(diffuseColor.rgb, uFrameColor, vFrame);',
      );
  };
  // Distinct cache key so three.js does not share the program with unpatched materials.
  material.customProgramCacheKey = () => 'pv-panel-v1';
  return material;
}
