/** A run of consecutive instance indices `[start, start + count)`. */
export interface Run {
  start: number;
  count: number;
}

/**
 * Coalesces instance indices into sorted runs of consecutive indices, so attribute uploads
 * touch only what changed. Beyond `maxRuns` runs, a single covering run is returned
 * (many tiny `bufferSubData` calls cost more than one larger upload).
 */
export function coalesceRuns(indices: Iterable<number>, maxRuns = 32): Run[] {
  const sorted = Uint32Array.from(indices).sort();
  const runs: Run[] = [];
  for (const i of sorted) {
    const last = runs[runs.length - 1];
    if (last && i <= last.start + last.count) {
      if (i === last.start + last.count) last.count += 1;
    } else {
      runs.push({ start: i, count: 1 });
    }
  }
  if (runs.length > maxRuns) {
    const first = runs[0];
    const last = runs[runs.length - 1];
    if (first && last)
      return [{ start: first.start, count: last.start + last.count - first.start }];
  }
  return runs;
}
