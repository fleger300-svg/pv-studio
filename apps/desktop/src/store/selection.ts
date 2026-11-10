import { create } from 'zustand';
import type { ProjectSummary } from '../bindings';

/**
 * Selected entities, shared by the 2D plan, the 3D view and the panels. Module selection
 * stays in the 3D editor (it is GPU-side and large).
 */
interface SelectionState {
  buildingId: string | null;
  faceId: string | null;
  selectBuilding: (buildingId: string | null) => void;
  /** Selects a roof face and its building. */
  selectFace: (faceId: string, buildingId: string) => void;
  clear: () => void;
  /** Drops what no longer exists after a reload. */
  prune: (summary: ProjectSummary | null) => void;
}

export const useSelectionStore = create<SelectionState>()((set, get) => ({
  buildingId: null,
  faceId: null,
  selectBuilding: (buildingId) => {
    set({ buildingId, faceId: null });
  },
  selectFace: (faceId, buildingId) => {
    set({ buildingId, faceId });
  },
  clear: () => {
    set({ buildingId: null, faceId: null });
  },
  prune: (summary) => {
    const { buildingId, faceId } = get();
    const building =
      buildingId !== null && summary?.buildings.some((b) => b.id === buildingId)
        ? buildingId
        : null;
    const face =
      faceId !== null && summary?.roofFaces.some((f) => f.face.id === faceId) ? faceId : null;
    if (building !== buildingId || face !== faceId) set({ buildingId: building, faceId: face });
  },
}));
