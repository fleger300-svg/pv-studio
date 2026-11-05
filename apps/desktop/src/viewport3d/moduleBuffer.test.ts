import { describe, expect, it } from 'vitest';
// Fixture written by the Rust encoder (`pv_store::buffer` test `typescript_fixture_is_up_to_date`).
import fixtureDataUrl from '../../../../testdata/fixtures/pvs1-fixture.bin?inline';
import expected from '../../../../testdata/fixtures/pvs1-fixture.json';
import { decodeModuleBuffer, FLAG_REMOVED, ModuleBufferError } from './moduleBuffer';

function fixtureBuffer(): ArrayBuffer {
  const base64 = fixtureDataUrl.slice(fixtureDataUrl.indexOf(',') + 1);
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  return bytes.buffer;
}

describe('PVS1 decoder', () => {
  it('decodes the Rust-encoded fixture exactly', () => {
    const data = decodeModuleBuffer(fixtureBuffer());
    expect(data.count).toBe(expected.length);
    expected.forEach((rec, i) => {
      expect(data.ids[i]).toBe(rec.id);
      expect(data.arrayIndex[i]).toBe(rec.arrayIndex);
      expect(data.flags[i]).toBe(rec.flags);
      // JSON values are the f32 values widened to f64, so equality is exact.
      expect(Array.from(data.positions.subarray(i * 3, i * 3 + 3))).toEqual(rec.position);
      expect(Array.from(data.rotations.subarray(i * 4, i * 4 + 4))).toEqual(rec.rotation);
    });
  });

  it('reads the removed flag', () => {
    const data = decodeModuleBuffer(fixtureBuffer());
    const removed = Array.from(data.flags).map((f) => (f & FLAG_REMOVED) !== 0);
    expect(removed).toEqual(expected.map((r) => (r.flags & FLAG_REMOVED) !== 0));
    expect(removed.filter(Boolean)).toHaveLength(1);
  });

  it('rejects malformed buffers', () => {
    expect(() => decodeModuleBuffer(new ArrayBuffer(4))).toThrow(ModuleBufferError);
    const bad = fixtureBuffer().slice(0);
    new DataView(bad).setUint32(0, 0, true);
    expect(() => decodeModuleBuffer(bad)).toThrow(/badMagic/);
    expect(() => decodeModuleBuffer(fixtureBuffer().slice(0, 40))).toThrow(/badLength/);
  });
});
