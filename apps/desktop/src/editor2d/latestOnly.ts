/**
 * Runs an async job for the latest request only: while one runs, newer requests replace
 * each other and only the last one runs next. Used for the roof preview during a drag
 * (one IPC call in flight; intermediate frames are skipped, never queued).
 */
export class LatestOnly<I, O> {
  private running = false;
  private pending: { input: I } | null = null;
  private generation = 0;

  constructor(
    private readonly job: (input: I) => Promise<O>,
    private readonly done: (output: O | Error, input: I) => void,
  ) {}

  request(input: I): void {
    this.pending = { input };
    if (!this.running) void this.pump();
  }

  /** Drops what is pending and ignores the result in flight. */
  cancel(): void {
    this.pending = null;
    this.generation += 1;
  }

  private async pump(): Promise<void> {
    while (this.pending) {
      const { input } = this.pending;
      this.pending = null;
      this.running = true;
      const generation = this.generation;
      let out: O | Error;
      try {
        out = await this.job(input);
      } catch (e) {
        out = e instanceof Error ? e : new Error(String(e));
      }
      this.running = false;
      if (generation === this.generation) this.done(out, input);
    }
  }
}
