import { describe, expect, it } from 'vitest';
import type { FieldStaticsResult_Serialize, ResultReady_Serialize } from '../bindings';
import { mergeReports, reduceReady, useStaticsStore, type FieldReport } from './statics';

const result: FieldStaticsResult_Serialize = {
  zones: ['interior'],
  decisions: [{ zone: 'interior', fixationSpacingM: 0.6, maxUtilisation: 0.4 }],
  fixations: 10,
  climbers: 12,
  ballastBlocks: null,
  maxUtilisation: 0.4,
  ok: true,
};

function report(arrayId: string, seq: number): FieldReport {
  return { arrayId, seq, inputFingerprint: seq.toString(16), outcome: { status: 'ok', result } };
}

function ready(seq: number, fields: FieldReport[], extra: Partial<ResultReady_Serialize> = {}) {
  return { seq, fields, bom: null, removed: [], reset: false, ...extra };
}

describe('statics reports', () => {
  it('keeps the most recent report of each field', () => {
    let fields = mergeReports({}, [report('a', 3), report('b', 1)]);
    fields = mergeReports(fields, [report('a', 2), report('b', 4)]);
    expect(fields.a?.seq).toBe(3); // older report ignored
    expect(fields.b?.seq).toBe(4);
  });

  it('returns the same object when nothing changes', () => {
    const fields = mergeReports({}, [report('a', 3)]);
    expect(mergeReports(fields, [report('a', 1)])).toBe(fields);
  });

  it('removes deleted fields and resets on a new project', () => {
    let fields = reduceReady({}, ready(1, [report('a', 1), report('b', 1)]));
    fields = reduceReady(fields, ready(2, [], { removed: ['b'] }));
    expect(Object.keys(fields)).toEqual(['a']);
    fields = reduceReady(fields, ready(3, [report('c', 3)], { reset: true }));
    expect(Object.keys(fields)).toEqual(['c']);
  });

  it('keeps a failed outcome with its translatable key', () => {
    const failed: FieldReport = {
      arrayId: 'a',
      seq: 5,
      inputFingerprint: '0',
      outcome: { status: 'failed', error: { key: 'errors.statics.siteMissing', params: {} } },
    };
    const fields = reduceReady({}, ready(5, [failed]));
    const outcome = fields.a?.outcome;
    expect(outcome?.status === 'failed' && outcome.error.key).toBe('errors.statics.siteMissing');
  });

  it('ignores a pass state older than the one held', () => {
    const s = useStaticsStore.getState();
    s.clear();
    s.applyState(true, 4);
    s.applySnapshot({ busy: false, seq: 2, fields: [report('a', 2)] });
    expect(useStaticsStore.getState().busy).toBe(true);
    expect(useStaticsStore.getState().fields.a?.seq).toBe(2);
    s.applyState(false, 4);
    expect(useStaticsStore.getState().busy).toBe(false);
  });
});
