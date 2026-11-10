import { create } from 'zustand';

/** Editing steps (the connection step comes later). */
export type Step = 'building' | 'roof' | 'layout';

export const STEPS: readonly Step[] = ['building', 'roof', 'layout'];

interface WorkflowState {
  step: Step;
  setStep: (step: Step) => void;
}

/** Active workflow step (UI state: it decides the layout of the centre and the panels). */
export const useWorkflowStore = create<WorkflowState>()((set) => ({
  step: 'layout',
  setStep: (step) => {
    set({ step });
  },
}));

/** The step a freshly opened project starts at: the layout when it has arrays, else the building. */
export function initialStep(arrayCount: number): Step {
  return arrayCount > 0 ? 'layout' : 'building';
}

/** Whether the step shows the 2D plan next to the 3D view. */
export function isPlanStep(step: Step): boolean {
  return step === 'building' || step === 'roof';
}
