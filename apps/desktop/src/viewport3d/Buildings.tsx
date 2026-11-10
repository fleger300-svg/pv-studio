import { useEffect, useMemo } from 'react';
import { DoubleSide, MeshStandardMaterial } from 'three';
import type { ProjectSummary } from '../bindings';
import { roofsGeometry, wallsGeometry } from './buildingGeometry';

/** Building walls (up to the roof, gables included) and roof faces: two draw calls in total. */
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
