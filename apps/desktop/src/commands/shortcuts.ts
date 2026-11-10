import { useEffect } from 'react';
import { usePerfStore } from '../perf/perfStore';
import { usePlanStore } from '../store/plan';
import { useSelectionStore } from '../store/selection';
import { useViewStore } from '../store/view';
import { useWorkflowStore } from '../store/workflow';
import { runCommand } from './runCommand';

/** Whether keyboard focus is in a text field (its own keys win). */
function typing(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    target.closest('input, select, textarea, [contenteditable="true"]') !== null
  );
}

/** Undo / redo from anywhere (2D plan, 3D view, panels). */
export function undo(): Promise<unknown> {
  return runCommand('undo', undefined, usePerfStore.getState().recordCommand);
}

export function redo(): Promise<unknown> {
  return runCommand('redo', undefined, usePerfStore.getState().recordCommand);
}

/**
 * Application shortcuts: Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z and Ctrl+Y everywhere; Delete and
 * Escape act on what the current step edits (modules in the layout, the selected building
 * on the plan).
 */
export function useShortcuts(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (typing(e.target)) return;
      const mod = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
      const layout = useWorkflowStore.getState().step === 'layout';
      const editor = useViewStore.getState().editor;
      // The plan's tools first (closing a polygon, removing a vertex, tool shortcuts…).
      if (!layout && !mod && usePlanStore.getState().keyHandler?.(e.key)) {
        e.preventDefault();
        return;
      }
      if (mod && key === 'z') {
        e.preventDefault();
        void (e.shiftKey ? redo() : undo());
      } else if (mod && key === 'y') {
        e.preventDefault();
        void redo();
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        if (layout) {
          void editor?.removeSelected();
          return;
        }
        const id = useSelectionStore.getState().buildingId;
        if (id) void runCommand('apply', { type: 'RemoveBuilding', id });
      } else if (e.key === 'Escape') {
        if (layout && editor) {
          if (editor.dragging) editor.cancelDrag();
          else editor.clearSelection();
        } else {
          useSelectionStore.getState().clear();
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, []);
}
