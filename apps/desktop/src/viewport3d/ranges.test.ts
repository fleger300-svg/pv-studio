import { describe, expect, it } from 'vitest';
import { coalesceRuns } from './ranges';

describe('coalesceRuns', () => {
  it('merges consecutive and duplicate indices', () => {
    expect(coalesceRuns([5, 3, 4, 4, 9, 10, 0])).toEqual([
      { start: 0, count: 1 },
      { start: 3, count: 3 },
      { start: 9, count: 2 },
    ]);
  });

  it('returns nothing for no indices', () => {
    expect(coalesceRuns([])).toEqual([]);
  });

  it('falls back to one covering run when fragmented', () => {
    expect(coalesceRuns([1, 3, 5, 7], 2)).toEqual([{ start: 1, count: 7 }]);
  });
});
