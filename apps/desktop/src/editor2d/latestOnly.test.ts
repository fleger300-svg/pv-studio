import { describe, expect, it } from 'vitest';
import { LatestOnly } from './latestOnly';

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('LatestOnly', () => {
  it('runs the first and the latest request, skipping those in between', async () => {
    const started: number[] = [];
    const results: number[] = [];
    const resolvers: (() => void)[] = [];
    const l = new LatestOnly<number, number>(
      (i) =>
        new Promise((resolve) => {
          started.push(i);
          resolvers.push(() => {
            resolve(i * 10);
          });
        }),
      (o) => {
        if (typeof o === 'number') results.push(o);
      },
    );
    l.request(1);
    l.request(2);
    l.request(3);
    await tick();
    expect(started).toEqual([1]);
    resolvers.shift()?.();
    await tick();
    expect(started).toEqual([1, 3]);
    resolvers.shift()?.();
    await tick();
    expect(results).toEqual([10, 30]);
  });

  it('ignores a result after cancel', async () => {
    const results: unknown[] = [];
    let release = () => {};
    const l = new LatestOnly<number, number>(
      () =>
        new Promise((resolve) => {
          release = () => {
            resolve(1);
          };
        }),
      (o) => results.push(o),
    );
    l.request(1);
    await tick();
    l.cancel();
    release();
    await tick();
    expect(results).toEqual([]);
  });
});
