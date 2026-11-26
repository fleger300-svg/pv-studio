import { useEffect, useMemo } from 'react';
import { DoubleSide, MeshStandardMaterial } from 'three';
import type { SurroundingsView } from '../bindings';
import { surroundingsGeometry } from './surroundingsGeometry';

/**
 * Terrain and neighbouring buildings of the surface model, as the shading sees them: they
 * cast and receive the sun's shadows. One draw call.
 */
export function Surroundings({ view, color }: { view: SurroundingsView; color: string }) {
  const geometry = useMemo(() => surroundingsGeometry(view), [view]);
  const material = useMemo(
    () => new MeshStandardMaterial({ color, roughness: 1, side: DoubleSide }),
    [color],
  );
  useEffect(
    () => () => {
      geometry?.dispose();
      material.dispose();
    },
    [geometry, material],
  );
  if (!geometry) return null;
  return (
    <mesh name="surroundings" geometry={geometry} material={material} castShadow receiveShadow />
  );
}
