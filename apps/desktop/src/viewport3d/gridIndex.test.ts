import { describe, expect, it } from 'vitest';
import { GridIndex } from './gridIndex';
import { FLAG_REMOVED } from './moduleBuffer';
import { flatRoofFixture } from './testSummary';

describe('grid-lookup selection', () => {
  it('finds the module under a roof point', () => {
    const { summary, data } = flatRoofFixture();
    const grid = new GridIndex(summary, data);
    expect(grid.hit(0, 1, 1.5)).toBe(0); // centre of row 0, col 0
    expect(grid.hit(0, 1 + 1.154 * 2 + 0.3, 1.5 + 1.782 - 0.5)).toBe(5); // inside row 1, col 2
  });

  it('returns null in gaps, outside the array and on other faces', () => {
    const { summary, data } = flatRoofFixture();
    const grid = new GridIndex(summary, data);
    expect(grid.hit(0, 1 + 1.154 / 2, 1.5)).toBeNull(); // 20 mm gap between columns
    expect(grid.hit(0, 9, 7)).toBeNull();
    expect(grid.hit(1, 1, 1.5)).toBeNull();
  });

  it('ignores removed modules', () => {
    const { summary, data } = flatRoofFixture();
    data.flags[4] = FLAG_REMOVED;
    const grid = new GridIndex(summary, data);
    expect(grid.hit(0, 1 + 1.154, 1.5 + 1.782)).toBeNull();
  });

  it('follows modules after a move', () => {
    const { summary, data } = flatRoofFixture();
    const grid = new GridIndex(summary, data);
    // Move module 0 by +3.0 m along u (onto a free area beyond column 2).
    data.positions[0] = (data.positions[0] ?? 0) + 3.5;
    grid.update([0]);
    expect(grid.hit(0, 1, 1.5)).toBeNull();
    expect(grid.hit(0, 4.5, 1.5)).toBe(0);
    expect(grid.hit(0, 1 + 1.154, 1.5)).toBe(1); // neighbours unaffected
  });
});
