import { useEffect, useMemo } from 'react';
import { useThree } from '@react-three/fiber';
import {
  DataTexture,
  DoubleSide,
  Mesh,
  MeshBasicMaterial,
  NearestFilter,
  PlaneGeometry,
  RGBAFormat,
  SRGBColorSpace,
} from 'three';
import type { ProjectSummary, RoofShadingMap } from '../bindings';
import { shadingColor } from '../store/shading';
import { roofMatrix } from './frames';

/** Height of the map over the roof [m]: over the covering, under the modules (the depth test
 *  keeps the modules in front). */
const LIFT_M = 0.03;
/** Opacity of the map. */
const OPACITY = 0.85;

/** RGBA pixels of a face grid: the shading colour of each cell, transparent off the face. */
export function gridPixels(losses: readonly number[]): Uint8Array {
  const px = new Uint8Array(losses.length * 4);
  const cache = new Map<number, [number, number, number]>();
  losses.forEach((x, i) => {
    if (x < 0) return;
    let rgb = cache.get(x);
    if (!rgb) {
      const n = Number.parseInt(shadingColor(x / 10000).slice(1), 16);
      rgb = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
      cache.set(x, rgb);
    }
    px.set([rgb[0], rgb[1], rgb[2], 255], i * 4);
  });
  return px;
}

/** Shading map of the roof faces, laid on them (one textured quad per face). */
export function RoofShading({ summary, map }: { summary: ProjectSummary; map: RoofShadingMap }) {
  const invalidate = useThree((s) => s.invalidate);
  const meshes = useMemo(() => {
    const out: Mesh[] = [];
    for (const g of map.faces) {
      const rf = summary.roofFaces.find((f) => f.face.id === g.faceId);
      if (!rf || g.cols === 0 || g.rows === 0) continue;
      const texture = new DataTexture(gridPixels(g.losses), g.cols, g.rows, RGBAFormat);
      texture.colorSpace = SRGBColorSpace;
      texture.magFilter = NearestFilter;
      texture.minFilter = NearestFilter;
      texture.needsUpdate = true;
      const w = g.cols * g.cellM;
      const h = g.rows * g.cellM;
      const geometry = new PlaneGeometry(w, h);
      geometry.translate(g.originUv[0] + w / 2, g.originUv[1] + h / 2, LIFT_M);
      const material = new MeshBasicMaterial({
        map: texture,
        transparent: true,
        opacity: OPACITY,
        depthWrite: false,
        side: DoubleSide,
      });
      const mesh = new Mesh(geometry, material);
      mesh.name = `roof-shading:${g.faceId}`;
      mesh.matrixAutoUpdate = false;
      mesh.matrix.copy(roofMatrix(rf.frame));
      out.push(mesh);
    }
    return out;
  }, [summary, map]);

  useEffect(() => {
    invalidate();
    return () => {
      for (const m of meshes) {
        m.geometry.dispose();
        const material = m.material as MeshBasicMaterial;
        material.map?.dispose();
        material.dispose();
      }
      invalidate();
    };
  }, [meshes, invalidate]);

  return (
    <group name="roof-shading">
      {meshes.map((m) => (
        <primitive key={m.name} object={m} />
      ))}
    </group>
  );
}
