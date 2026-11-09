import type { ProjectSummary } from '../bindings';
import type { ModuleData } from './moduleBuffer';

/**
 * Test fixture: one flat 10 m × 8 m roof at z = 3 (u = east, v = north) with a 2 × 3 grid
 * of portrait modules (pitch 1.154 × 1.782 m). Module ids are row-major.
 */
export function flatRoofFixture(): { summary: ProjectSummary; data: ModuleData } {
  const grid = {
    rows: 2,
    cols: 3,
    originU: 1,
    originV: 1.5,
    pitchU: 1.154,
    pitchV: 1.782,
    blockCols: 0,
    blockRows: 0,
    blockExtraU: 0,
    blockExtraV: 0,
  };
  const summary: ProjectSummary = {
    projectId: 'p',
    name: 'test',
    path: 'test.pvproj',
    moduleCount: 6,
    activeModuleCount: 6,
    modulesPerString: 18,
    moduleThicknessM: 0.03,
    modulePeakPowerWp: 430,
    buildings: [],
    roofFaces: [
      {
        face: {
          id: 'f',
          buildingId: 'b',
          name: 'Flat',
          origin: [0, 0, 3],
          tiltDeg: 0,
          azimuthDeg: 180,
          widthM: 10,
          heightM: 8,
        },
        frame: { origin: [0, 0, 3], axisU: [1, 0, 0], axisV: [0, 1, 0], normal: [0, 0, 1] },
      },
    ],
    arrays: [
      {
        array: {
          id: 'a',
          roofFaceId: 'f',
          moduleLengthM: 1.762,
          moduleWidthM: 1.134,
          orientation: 'portrait',
          tiltOnRoofDeg: 0,
          azimuthOffsetDeg: 0,
          grid,
          layout: {
            boundaryUv: [],
            moduleGapUM: 0,
            moduleGapVM: 0,
            edgeSetbackM: 0,
            edgeSetbacks: [],
            obstacleClearanceM: 0,
            anchor: 'lowerLeft',
            expansionJoints: [],
            minDistanceToOtherArraysM: 0,
          },
        },
        roofFaceIndex: 0,
        firstModuleId: 0,
        moduleCount: 6,
        moduleSize: [1.134, 1.762, 0.03],
        halfFootprint: [0.567, 0.881],
      },
    ],
    walls: [],
    canUndo: false,
    canRedo: false,
  };
  const count = 6;
  const positions = new Float32Array(count * 3);
  for (let id = 0; id < count; id++) {
    const row = Math.floor(id / 3);
    const col = id % 3;
    positions.set(
      [grid.originU + col * grid.pitchU, grid.originV + row * grid.pitchV, 3.065],
      id * 3,
    );
  }
  const rotations = new Float32Array(count * 4);
  for (let id = 0; id < count; id++) rotations[id * 4 + 3] = 1;
  const data: ModuleData = {
    count,
    ids: Uint32Array.from({ length: count }, (_, i) => i),
    arrayIndex: new Uint16Array(count),
    flags: new Uint16Array(count),
    positions,
    rotations,
  };
  return { summary, data };
}
