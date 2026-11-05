/**
 * Shared frame recorder, fed by `<FrameProbe>` inside the R3F loop (outside React state so
 * recording costs nothing per frame).
 */
export interface FrameTask {
  /** Called before each render with `performance.now()`; return `false` when done. */
  (now: number): boolean;
}

const LIVE_WINDOW_MS = 2000;

export const frameRecorder = {
  /** Timestamps of recently rendered frames (last ~2 s). */
  live: [] as number[],
  /** Renderer counters of the last rendered frame (`gl.info.render`). */
  calls: 0,
  triangles: 0,
  /** Benchmark step driven frame by frame (invalidates every frame while active). */
  task: null as FrameTask | null,

  record(now: number, calls: number, triangles: number): void {
    this.live.push(now);
    while (this.live.length > 0 && (this.live[0] ?? 0) < now - LIVE_WINDOW_MS) this.live.shift();
    this.calls = calls;
    this.triangles = triangles;
  },

  /** Runs `task` once per rendered frame until it returns false. */
  run(task: FrameTask, kick: () => void): Promise<void> {
    return new Promise((resolve) => {
      this.task = (now) => {
        const more = task(now);
        if (!more) {
          this.task = null;
          resolve();
        }
        return more;
      };
      kick();
    });
  },
};
