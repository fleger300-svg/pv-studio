import { useEffect, useMemo } from 'react';
import {
  BoxGeometry,
  BufferGeometry,
  Color,
  DoubleSide,
  Float32BufferAttribute,
  MeshStandardMaterial,
  PlaneGeometry,
} from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { ProjectSummary } from '../bindings';
import { roofMatrix } from './frames';

/** Walls stop just below the eave so flat roofs do not z-fight with the box top. */
const WALL_TOP_GAP_M = 0.02;

function wallsGeometry(summary: ProjectSummary): BufferGeometry | null {
  const parts: BufferGeometry[] = [];
  for (const b of summary.buildings) {
    const [x, y] = b.origin;
    const [sx, sy] = b.sizeM;
    const h = b.eaveHeightM - WALL_TOP_GAP_M;
    const box = new BoxGeometry(sx, sy, h).toNonIndexed();
    box.translate(x, y, h / 2);
    box.deleteAttribute('uv');
    parts.push(box);
    if (b.roofKind === 'gable') {
      // Triangular gable ends on the east and west walls (ridge runs along x).
      const tri: number[] = [];
      for (const gx of [x - sx / 2, x + sx / 2]) {
        tri.push(
          gx,
          y - sy / 2,
          b.eaveHeightM,
          gx,
          y + sy / 2,
          b.eaveHeightM,
          gx,
          y,
          b.ridgeHeightM,
        );
      }
      const gable = new BufferGeometry();
      gable.setAttribute('position', new Float32BufferAttribute(tri, 3));
      gable.computeVertexNormals();
      parts.push(gable);
    }
  }
  return parts.length ? mergeGeometries(parts) : null;
}

const ROOF_COLORS = { gable: new Color('#8d5a4a'), flat: new Color('#8f969c') } as const;

function roofsGeometry(summary: ProjectSummary): BufferGeometry | null {
  const kindOf = new Map(summary.buildings.map((b) => [b.id, b.roofKind]));
  const parts = summary.roofFaces.map(({ face, frame }) => {
    const plane = new PlaneGeometry(face.widthM, face.heightM);
    plane.translate(face.widthM / 2, face.heightM / 2, 0);
    plane.applyMatrix4(roofMatrix(frame));
    const color = ROOF_COLORS[kindOf.get(face.buildingId) ?? 'gable'];
    const count = plane.getAttribute('position').count;
    const colors = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) color.toArray(colors, i * 3);
    plane.setAttribute('color', new Float32BufferAttribute(colors, 3));
    return plane;
  });
  return parts.length ? mergeGeometries(parts) : null;
}

/** Simple building volumes (walls + gables) and roof planes: two draw calls in total. */
export function Buildings({ summary }: { summary: ProjectSummary }) {
  const walls = useMemo(() => wallsGeometry(summary), [summary]);
  const roofs = useMemo(() => roofsGeometry(summary), [summary]);
  const wallMaterial = useMemo(
    () => new MeshStandardMaterial({ color: '#e7e2d9', roughness: 0.9, side: DoubleSide }),
    [],
  );
  const roofMaterial = useMemo(
    () => new MeshStandardMaterial({ vertexColors: true, roughness: 0.8, side: DoubleSide }),
    [],
  );
  useEffect(
    () => () => {
      walls?.dispose();
      roofs?.dispose();
      wallMaterial.dispose();
      roofMaterial.dispose();
    },
    [walls, roofs, wallMaterial, roofMaterial],
  );
  return (
    <group name="buildings">
      {walls && <mesh geometry={walls} material={wallMaterial} castShadow receiveShadow />}
      {roofs && <mesh geometry={roofs} material={roofMaterial} castShadow receiveShadow />}
    </group>
  );
}
