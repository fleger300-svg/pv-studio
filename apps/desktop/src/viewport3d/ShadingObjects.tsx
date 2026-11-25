import { useEffect, useMemo } from 'react';
import { DoubleSide, MeshStandardMaterial } from 'three';
import type { ShadingObject } from '../bindings';
import { objectsGeometry } from './objectGeometry';

/**
 * Shading objects around the buildings (neighbouring buildings, roof fixtures, walls,
 * polygons, trees): two draw calls, the trees' crowns semi-transparent.
 */
export function ShadingObjects({ objects }: { objects: readonly ShadingObject[] }) {
  const { solids, crowns } = useMemo(() => objectsGeometry(objects), [objects]);
  const solidMaterial = useMemo(
    () => new MeshStandardMaterial({ vertexColors: true, roughness: 0.85, side: DoubleSide }),
    [],
  );
  const crownMaterial = useMemo(
    () =>
      new MeshStandardMaterial({
        vertexColors: true,
        roughness: 1,
        transparent: true,
        opacity: 0.7,
        depthWrite: false,
        side: DoubleSide,
      }),
    [],
  );
  useEffect(
    () => () => {
      solids?.dispose();
      crowns?.dispose();
      solidMaterial.dispose();
      crownMaterial.dispose();
    },
    [solids, crowns, solidMaterial, crownMaterial],
  );
  return (
    <group name="shading-objects">
      {solids && <mesh geometry={solids} material={solidMaterial} castShadow receiveShadow />}
      {crowns && <mesh geometry={crowns} material={crownMaterial} castShadow />}
    </group>
  );
}
