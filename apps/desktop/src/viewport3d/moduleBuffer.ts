/**
 * Decoder of the `PVS1` binary module buffer produced by `pv_store::buffer` (Rust).
 * Keep in sync with the Rust doc comment. All values are little-endian.
 *
 * Header (12 bytes): magic u32 = 0x50565331, version u16 = 1, reserved u16, count u32.
 * Record (36 bytes): id u32, arrayIndex u16, flags u16 (bit 0 = removed),
 *                    position x,y,z f32 (world ENU, m), rotation qx,qy,qz,qw f32.
 */
export const PVS1_MAGIC = 0x50565331;
export const PVS1_VERSION = 1;
export const HEADER_LEN = 12;
export const RECORD_LEN = 36;
export const FLAG_REMOVED = 1;

/** Struct-of-arrays view of the modules, ready for GPU upload. */
export interface ModuleData {
  count: number;
  ids: Uint32Array;
  arrayIndex: Uint16Array;
  flags: Uint16Array;
  /** xyz per module. */
  positions: Float32Array;
  /** xyzw per module. */
  rotations: Float32Array;
}

export class ModuleBufferError extends Error {
  constructor(readonly reason: 'tooShort' | 'badMagic' | 'badVersion' | 'badLength') {
    super(`invalid PVS1 buffer: ${reason}`);
    this.name = 'ModuleBufferError';
  }
}

export function decodeModuleBuffer(buffer: ArrayBuffer): ModuleData {
  if (buffer.byteLength < HEADER_LEN) throw new ModuleBufferError('tooShort');
  const view = new DataView(buffer);
  if (view.getUint32(0, true) !== PVS1_MAGIC) throw new ModuleBufferError('badMagic');
  if (view.getUint16(4, true) !== PVS1_VERSION) throw new ModuleBufferError('badVersion');
  const count = view.getUint32(8, true);
  if (buffer.byteLength !== HEADER_LEN + count * RECORD_LEN) {
    throw new ModuleBufferError('badLength');
  }

  const ids = new Uint32Array(count);
  const arrayIndex = new Uint16Array(count);
  const flags = new Uint16Array(count);
  const positions = new Float32Array(count * 3);
  const rotations = new Float32Array(count * 4);
  for (let i = 0, o = HEADER_LEN; i < count; i++, o += RECORD_LEN) {
    ids[i] = view.getUint32(o, true);
    arrayIndex[i] = view.getUint16(o + 4, true);
    flags[i] = view.getUint16(o + 6, true);
    positions[i * 3] = view.getFloat32(o + 8, true);
    positions[i * 3 + 1] = view.getFloat32(o + 12, true);
    positions[i * 3 + 2] = view.getFloat32(o + 16, true);
    rotations[i * 4] = view.getFloat32(o + 20, true);
    rotations[i * 4 + 1] = view.getFloat32(o + 24, true);
    rotations[i * 4 + 2] = view.getFloat32(o + 28, true);
    rotations[i * 4 + 3] = view.getFloat32(o + 32, true);
  }
  return { count, ids, arrayIndex, flags, positions, rotations };
}
