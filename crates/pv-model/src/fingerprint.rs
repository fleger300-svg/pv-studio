//! Stable 64-bit fingerprints (FNV-1a) of serialisable data.
//!
//! Derived results carry the fingerprint of their inputs (ADR-004, architecture: « Statique,
//! strings, simulation et nomenclature portent l'empreinte (hash) de leurs entrées ») so that
//! caches only recompute what actually changed. The dependency graph (`pv-graph`) uses the
//! same function for its early cut-off.

use serde::Serialize;

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0100_0000_01b3;

/// FNV-1a of raw bytes.
#[must_use]
pub fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(OFFSET, |hash, &b| (hash ^ u64::from(b)).wrapping_mul(PRIME))
}

/// Fingerprint of any serialisable value.
///
/// Serialisation goes through `serde_json`, which keeps struct field order, so the hash is
/// stable across runs for the same input.
#[must_use]
pub fn fingerprint<T: Serialize + ?Sized>(input: &T) -> u64 {
    // Serialising plain data structs cannot fail; fall back to an empty payload otherwise.
    fingerprint_bytes(&serde_json::to_vec(input).unwrap_or_default())
}

/// Combines fingerprints in order (order-sensitive), e.g. the fingerprints of a node's
/// dependencies.
#[must_use]
pub fn combine(parts: impl IntoIterator<Item = u64>) -> u64 {
    parts.into_iter().fold(OFFSET, |hash, part| {
        part.to_le_bytes()
            .iter()
            .fold(hash, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_input_sensitive() {
        #[derive(Serialize)]
        struct In {
            a: f64,
        }
        assert_eq!(fingerprint(&In { a: 1.0 }), fingerprint(&In { a: 1.0 }));
        assert_ne!(fingerprint(&In { a: 1.0 }), fingerprint(&In { a: 1.5 }));
        // Known FNV-1a 64 vector.
        assert_eq!(fingerprint_bytes(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn combine_is_order_sensitive() {
        assert_eq!(combine([1, 2]), combine([1, 2]));
        assert_ne!(combine([1, 2]), combine([2, 1]));
    }
}
