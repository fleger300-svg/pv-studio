import { commands, type CommandError } from '../bindings';

/**
 * Single entry point to the Rust back-end. Wraps the generated `commands` to count IPC
 * calls (the benchmark asserts that a drag performs none) and to unwrap results.
 */
let callCount = 0;

/** Number of IPC calls made since start-up. */
export function ipcCallCount(): number {
  return callCount;
}

type Commands = typeof commands;

export const api: Commands = Object.fromEntries(
  Object.entries(commands).map(([name, fn]) => [
    name,
    (...args: unknown[]) => {
      callCount += 1;
      return (fn as (...a: unknown[]) => unknown)(...args);
    },
  ]),
) as Commands;

/** A translatable back-end error (`key` + ICU `params`). */
export class BackendError extends Error {
  readonly key: string;
  readonly params: CommandError['params'];

  constructor(error: CommandError) {
    super(error.key);
    this.name = 'BackendError';
    this.key = error.key;
    this.params = error.params;
  }
}

type Result<T> = { status: 'ok'; data: T } | { status: 'error'; error: CommandError };

/** Returns the data of a command result or throws a {@link BackendError}. */
export async function unwrap<T>(result: Promise<Result<T>>): Promise<T> {
  const r = await result;
  if (r.status === 'error') throw new BackendError(r.error);
  return r.data;
}
