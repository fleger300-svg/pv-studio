import { useEffect, useMemo } from 'react';
import { DoubleSide, MeshStandardMaterial } from 'three';
import type { ProjectSummary } from '../bindings';
import { obstaclesGeometry, roofsGeometry, wallsGeometry } from './buildingGeometry';

/**
 * Building walls (up to the roof, gables included), roof faces and roof obstacles: three
 * draw calls in total.
 */
export function Buildings({ summary }: { summary: ProjectSummary }) {
  const walls = useMemo(() => wallsGeometry(summary), [summary]);
  const roofs = useMemo(() => roofsGeometry(summary), [summary]);
  const obstacles = useMemo(() => obstaclesGeometry(summary), [summary]);
  const wallMaterial = useMemo(
    () => new MeshStandardMaterial({ color: '#e7e2d9', roughness: 0.9, side: DoubleSide }),
    [],
  );
  const roofMaterial = useMemo(
    () => new MeshStandardMaterial({ vertexColors: true, roughness: 0.8, side: DoubleSide }),
    [],
  );
  const obstacleMaterial = useMemo(
    () => new MeshStandardMaterial({ color: '#5b5f66', roughness: 0.7 }),
    [],
  );
  useEffect(
    () => () => {
      walls?.dispose();
      roofs?.dispose();
      obstacles?.dispose();
      wallMaterial.dispose();
      roofMaterial.dispose();
      obstacleMaterial.dispose();
    },
    [walls, roofs, obstacles, wallMaterial, roofMaterial, obstacleMaterial],
  );
  return (
    <group name="buildings">
      {walls && <mesh geometry={walls} material={wallMaterial} castShadow receiveShadow />}
      {roofs && <mesh geometry={roofs} material={roofMaterial} castShadow receiveShadow />}
      {obstacles && (
        <mesh geometry={obstacles} material={obstacleMaterial} castShadow receiveShadow />
      )}
    </group>
  );
}
