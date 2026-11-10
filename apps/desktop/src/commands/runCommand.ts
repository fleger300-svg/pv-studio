import type { ApplyResult, Command, CommandError } from '../bindings';
import { api, BackendError, unwrap } from '../ipc';
import { useProjectStore } from '../store/project';
import { useToastStore } from '../store/toast';

/** Timing of one command round trip, reported to the performance HUD. */
export interface CommandTiming {
  kind: 'apply' | 'undo' | 'redo';
  /** Front-measured invoke → result, ms. */
  roundTripMs: number;
  /** Time spent in Rust, µs (from `ApplyResult`). */
  rustUs: number;
}

let busy = false;

/** Whether a command is in flight (commands are sent one at a time). */
export function commandBusy(): boolean {
  return busy;
}

/** A failure as a translatable error (never prose). */
export function toCommandError(e: unknown): CommandError {
  return e instanceof BackendError
    ? { key: e.key, params: e.params }
    : { key: 'errors.store.failure', params: { detail: String(e) } };
}

/**
 * The single path of every model change from the UI: applies a command (or undo / redo),
 * updates the history counters, reports failures as toasts. One command at a time: a call
 * made while another is in flight is dropped (`null`), like a double click.
 *
 * Returns the result, or `null` when the command was dropped or refused.
 */
export async function runCommand(
  kind: CommandTiming['kind'],
  cmd?: Command,
  onTiming?: ((t: CommandTiming) => void) | null,
): Promise<ApplyResult | null> {
  if (kind === 'apply' && !cmd) return null;
  return runApplied(
    kind,
    () =>
      kind === 'apply' && cmd
        ? unwrap(api.applyCommand(cmd))
        : kind === 'undo'
          ? unwrap(api.undo())
          : unwrap(api.redo()),
    onTiming,
  );
}

/**
 * Same path for back-end calls that build and apply their command in Rust (a layout of
 * thousands of modules is not sent to the front-end and back).
 */
export async function runApplied(
  kind: CommandTiming['kind'],
  call: () => Promise<ApplyResult>,
  onTiming?: ((t: CommandTiming) => void) | null,
): Promise<ApplyResult | null> {
  if (busy) return null;
  busy = true;
  const started = performance.now();
  try {
    const result = await call();
    onTiming?.({ kind, roundTripMs: performance.now() - started, rustUs: result.rustUs });
    useProjectStore.getState().setCounters({
      canUndo: result.canUndo,
      canRedo: result.canRedo,
      activeModuleCount: result.activeModuleCount,
    });
    return result;
  } catch (e) {
    useToastStore.getState().push(toCommandError(e));
    return null;
  } finally {
    busy = false;
  }
}
