import { describe, expect, it } from 'vitest';
import { nextBom, type BomReport } from './bom';

function report(seq: number, price: number): BomReport {
  const table = { faceId: null, title: 'total', lines: [], massKg: 0, price };
  return {
    seq,
    inputFingerprint: seq.toString(16),
    outcome: { status: 'ok', bom: { faces: [], total: table, savedBars: [] } },
  };
}

describe('bill of materials report', () => {
  it('keeps the most recent report', () => {
    const r3 = report(3, 30);
    expect(nextBom(null, r3, false)).toBe(r3);
    expect(nextBom(r3, report(2, 20), false)).toBe(r3); // older: ignored
    const r5 = report(5, 50);
    expect(nextBom(r3, r5, false)).toBe(r5);
  });

  it('keeps the held report when the BOM did not change', () => {
    const r3 = report(3, 30);
    expect(nextBom(r3, null, false)).toBe(r3);
  });

  it('drops the held report on a new project', () => {
    expect(nextBom(report(9, 90), null, true)).toBeNull();
    const r1 = report(1, 10);
    expect(nextBom(report(9, 90), r1, true)).toBe(r1);
  });
});
