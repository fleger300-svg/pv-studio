//! Binary module buffer (`PVS1`), sent to the front-end as raw bytes over the IPC.
//!
//! All values are **little-endian**. Mirrored in TypeScript by
//! `apps/desktop/src/viewport3d/moduleBuffer.ts` — keep both in sync.
//!
//! ```text
//! Header (12 bytes)
//!   0  u32  magic    = 0x50565331 ("PVS1")
//!   4  u16  version  = 1
//!   6  u16  reserved = 0
//!   8  u32  count    = number of module records
//! Record (36 bytes each, starting at byte 12)
//!   0  u32  id           module id = global GPU instance index
//!   4  u16  array_index  index into ProjectSummary.arrays
//!   6  u16  flags        bit 0 = removed; other bits reserved (0)
//!   8  f32  x, y, z      module centre, world ENU, metres
//!  20  f32  qx, qy, qz, qw  world rotation (unit quaternion)
//! ```
//!
//! Note: the record is 36 bytes (the sum of the listed fields), not 32.

use pv_model::{ModuleId, ModuleState, Project, Quat};

pub const MAGIC: u32 = 0x5056_5331;
pub const VERSION: u16 = 1;
pub const HEADER_LEN: usize = 12;
pub const RECORD_LEN: usize = 36;
pub const FLAG_REMOVED: u16 = 1 << 0;

/// One decoded record (used by tests and tooling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModuleRecord {
    pub id: u32,
    pub array_index: u16,
    pub flags: u16,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BufferError {
    #[error("buffer too short")]
    TooShort,
    #[error("bad magic {0:#010x}")]
    BadMagic(u32),
    #[error("unsupported version {0}")]
    UnsupportedVersion(u16),
    #[error("length mismatch: header says {count} records, buffer holds {actual} bytes")]
    LengthMismatch { count: u32, actual: usize },
}

/// Precomputed per-array transform data, so encoding does no trigonometry per module.
struct ArrayXf {
    frame: pv_model::RoofFrame,
    rotation: Quat,
    offset: f64,
}

fn array_transforms(project: &Project) -> Vec<Option<ArrayXf>> {
    project
        .arrays
        .iter()
        .map(|a| {
            let face = project.roof_face(a.roof_face_id)?;
            let frame = face.frame();
            Some(ArrayXf {
                frame,
                rotation: (frame.rotation() * a.rotation_in_roof()).normalized(),
                offset: a.centre_offset(),
            })
        })
        .collect()
}

/// Encodes all modules (`ids = None`) or the given subset, in the given order.
/// Unknown ids are skipped.
#[must_use]
pub fn encode(project: &Project, ids: Option<&[ModuleId]>) -> Vec<u8> {
    let xfs = array_transforms(project);
    let array_index: std::collections::HashMap<_, _> = project
        .arrays
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();

    let selected: Box<dyn Iterator<Item = &pv_model::ModuleInstance>> = match ids {
        None => Box::new(project.modules.iter()),
        Some(ids) => Box::new(ids.iter().filter_map(|id| project.module(*id))),
    };
    let modules: Vec<_> = selected
        .filter_map(|m| {
            let ai = *array_index.get(&m.array_id)?;
            let xf = xfs[ai].as_ref()?;
            Some((m, ai, xf))
        })
        .collect();

    let mut out = Vec::with_capacity(HEADER_LEN + modules.len() * RECORD_LEN);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(modules.len() as u32).to_le_bytes());
    for (m, ai, xf) in modules {
        let p = xf.frame.to_world(m.u, m.v, xf.offset);
        let q = xf.rotation;
        let flags = if m.state == ModuleState::Removed {
            FLAG_REMOVED
        } else {
            0
        };
        out.extend_from_slice(&m.id.0.to_le_bytes());
        out.extend_from_slice(&(ai as u16).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        for c in [p[0], p[1], p[2], q.x, q.y, q.z, q.w] {
            out.extend_from_slice(&(c as f32).to_le_bytes());
        }
    }
    out
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Decodes a `PVS1` buffer.
///
/// # Errors
/// On malformed input.
pub fn decode(bytes: &[u8]) -> Result<Vec<ModuleRecord>, BufferError> {
    if bytes.len() < HEADER_LEN {
        return Err(BufferError::TooShort);
    }
    let magic = u32_at(bytes, 0);
    if magic != MAGIC {
        return Err(BufferError::BadMagic(magic));
    }
    let version = u16_at(bytes, 4);
    if version != VERSION {
        return Err(BufferError::UnsupportedVersion(version));
    }
    let count = u32_at(bytes, 8);
    if bytes.len() != HEADER_LEN + count as usize * RECORD_LEN {
        return Err(BufferError::LengthMismatch {
            count,
            actual: bytes.len(),
        });
    }
    Ok((0..count as usize)
        .map(|i| {
            let o = HEADER_LEN + i * RECORD_LEN;
            ModuleRecord {
                id: u32_at(bytes, o),
                array_index: u16_at(bytes, o + 4),
                flags: u16_at(bytes, o + 6),
                position: [
                    f32_at(bytes, o + 8),
                    f32_at(bytes, o + 12),
                    f32_at(bytes, o + 16),
                ],
                rotation: [
                    f32_at(bytes, o + 20),
                    f32_at(bytes, o + 24),
                    f32_at(bytes, o + 28),
                    f32_at(bytes, o + 32),
                ],
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_project;

    #[test]
    fn header_layout() {
        let p = fixture_project();
        let buf = encode(&p, None);
        assert_eq!(&buf[0..4], &[0x31, 0x53, 0x56, 0x50], "magic LE");
        assert_eq!(u16_at(&buf, 4), 1);
        assert_eq!(u16_at(&buf, 6), 0);
        assert_eq!(u32_at(&buf, 8) as usize, p.modules.len());
        assert_eq!(buf.len(), HEADER_LEN + p.modules.len() * RECORD_LEN);
    }

    #[test]
    fn encode_decode_round_trip() {
        let p = fixture_project();
        let records = decode(&encode(&p, None)).expect("decode");
        assert_eq!(records.len(), p.modules.len());
        for (r, m) in records.iter().zip(&p.modules) {
            assert_eq!(r.id, m.id.0);
            let (pos, rot) = p.module_transform(m).expect("transform");
            for (got, want) in r.position.iter().zip(pos) {
                assert!((f64::from(*got) - want).abs() < 1e-4);
            }
            for (got, want) in r.rotation.iter().zip([rot.x, rot.y, rot.z, rot.w]) {
                assert!((f64::from(*got) - want).abs() < 1e-6);
            }
            let removed = m.state == ModuleState::Removed;
            assert_eq!(r.flags & FLAG_REMOVED != 0, removed);
            assert_eq!(p.arrays[usize::from(r.array_index)].id, m.array_id);
        }
    }

    #[test]
    fn subset_encoding() {
        let p = fixture_project();
        let records = decode(&encode(
            &p,
            Some(&[ModuleId(3), ModuleId(999), ModuleId(1)]),
        ))
        .expect("decode");
        assert_eq!(records.iter().map(|r| r.id).collect::<Vec<_>>(), [3, 1]);
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(decode(&[0; 4]), Err(BufferError::TooShort));
        let mut buf = encode(&fixture_project(), None);
        buf[0] = 0;
        assert!(matches!(decode(&buf), Err(BufferError::BadMagic(_))));
        let mut buf = encode(&fixture_project(), None);
        buf.pop();
        assert!(matches!(
            decode(&buf),
            Err(BufferError::LengthMismatch { .. })
        ));
    }

    /// Writes (with `UPDATE_FIXTURES=1`) or verifies the fixture consumed by the TypeScript
    /// decoder test, so that both implementations agree byte for byte.
    #[test]
    fn typescript_fixture_is_up_to_date() {
        let p = fixture_project();
        let bin = encode(&p, None);
        let expected: Vec<serde_json::Value> = decode(&bin)
            .expect("decode")
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "arrayIndex": r.array_index,
                    "flags": r.flags,
                    "position": r.position,
                    "rotation": r.rotation,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&expected).expect("json") + "\n";

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures");
        let bin_path = dir.join("pvs1-fixture.bin");
        let json_path = dir.join("pvs1-fixture.json");
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(&bin_path, &bin).expect("write bin");
            std::fs::write(&json_path, &json).expect("write json");
        }
        let on_disk = std::fs::read(&bin_path)
            .expect("fixture missing: run `UPDATE_FIXTURES=1 cargo test -p pv-store`");
        assert_eq!(
            on_disk, bin,
            "binary fixture out of date (UPDATE_FIXTURES=1)"
        );
        let on_disk_json = std::fs::read_to_string(&json_path).expect("json fixture");
        assert_eq!(
            on_disk_json.replace("\r\n", "\n"),
            json,
            "JSON fixture out of date"
        );
    }
}
