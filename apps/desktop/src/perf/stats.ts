/** `p`-th percentile (0–100) of `values`, nearest-rank method. */
export function percentile(values: readonly number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[rank] ?? 0;
}

export interface FrameSummary {
  frames: number;
  durationS: number;
  avgFps: number;
  /** Lowest frame count over the complete 1-second windows of the run. */
  minFps: number;
  p95FrameMs: number;
  maxFrameMs: number;
  /** Median frame interval (≈ display refresh period when not GPU-bound). */
  medianFrameMs: number;
}

/** Summarises rendered-frame timestamps (ms, ascending). */
export function summarizeFrames(timestamps: readonly number[]): FrameSummary {
  const n = timestamps.length;
  const first = timestamps[0] ?? 0;
  const last = timestamps[n - 1] ?? 0;
  const durationMs = last - first;
  const intervals: number[] = [];
  for (let i = 1; i < n; i++) intervals.push((timestamps[i] ?? 0) - (timestamps[i - 1] ?? 0));
  const buckets: number[] = [];
  for (let w = first; w + 1000 <= last + 1e-6; w += 1000) {
    buckets.push(timestamps.filter((t) => t >= w && t < w + 1000).length);
  }
  return {
    frames: n,
    durationS: durationMs / 1000,
    avgFps: durationMs > 0 ? ((n - 1) * 1000) / durationMs : 0,
    minFps: buckets.length ? Math.min(...buckets) : 0,
    p95FrameMs: percentile(intervals, 95),
    maxFrameMs: intervals.length ? Math.max(...intervals) : 0,
    medianFrameMs: percentile(intervals, 50),
  };
}

/** FPS over the last second before `now` (for the live HUD). */
export function rollingFps(timestamps: readonly number[], now: number): number {
  return timestamps.filter((t) => t > now - 1000 && t <= now).length;
}
