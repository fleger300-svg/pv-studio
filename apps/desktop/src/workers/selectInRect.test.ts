import { describe, expect, it } from 'vitest';
import { Matrix4, PerspectiveCamera, Vector3 } from 'three';
import { selectInRect } from './selectInRect';

const IDENTITY = new Matrix4().elements;

describe('selectInRect', () => {
  it('selects centres inside the rectangle (any corner order)', () => {
    const positions = Float32Array.from([0, 0, 0, 0.5, 0.5, 0, -0.9, 0.2, 0, 0.3, -0.8, 0]);
    expect(
      Array.from(selectInRect(positions, 4, IDENTITY, { x0: 0.6, y0: 0.6, x1: -0.1, y1: -0.1 })),
    ).toEqual([0, 1]);
  });

  it('honours the skip predicate (removed modules)', () => {
    const positions = Float32Array.from([0, 0, 0, 0.1, 0, 0]);
    const ids = selectInRect(
      positions,
      2,
      IDENTITY,
      { x0: -1, y0: -1, x1: 1, y1: 1 },
      (id) => id === 0,
    );
    expect(Array.from(ids)).toEqual([1]);
  });

  it('uses the camera view-projection and ignores points behind it', () => {
    const camera = new PerspectiveCamera(60, 1, 0.1, 100);
    camera.position.set(0, 0, 10);
    camera.lookAt(new Vector3(0, 0, 0));
    camera.updateMatrixWorld();
    const vp = new Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
    // In front at the centre, in front far off-screen, behind the camera.
    const positions = Float32Array.from([0, 0, 0, 50, 0, 0, 0, 0, 20]);
    expect(
      Array.from(selectInRect(positions, 3, vp.elements, { x0: -0.5, y0: -0.5, x1: 0.5, y1: 0.5 })),
    ).toEqual([0]);
  });
});
