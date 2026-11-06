//! Node identities: a kind (what is computed) and an instance (for which entity).

use std::fmt;
use std::hash::{Hash, Hasher};

use pv_model::{ArrayId, BuildingId, RoofFaceId};
use serde::Serialize;

/// Name of a node kind, unique within a graph (`"layout"`, `"wind_zones"`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct KindId(pub &'static str);

/// Hashes the length and the first 8 bytes only (consistent with `Eq`: equal names hash
/// equally). Kind names are few and distinct; the instance disambiguates the rest of the key,
/// and node lookups are the engine's hot path.
impl Hash for KindId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let b = self.0.as_bytes();
        let mut w = [0u8; 8];
        let n = b.len().min(8);
        w[..n].copy_from_slice(&b[..n]);
        state.write_u64(u64::from_le_bytes(w) ^ ((b.len() as u64) << 56));
    }
}

impl fmt::Display for KindId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The entity a node is computed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(tag = "scope", content = "id", rename_all = "camelCase")]
pub enum Instance {
    Project,
    Building(BuildingId),
    Face(RoofFaceId),
    Array(ArrayId),
    /// Generic index (configuration slots, tests).
    Index(u64),
}

impl fmt::Display for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instance::Project => f.write_str("project"),
            Instance::Building(id) => write!(f, "building:{}", id.0),
            Instance::Face(id) => write!(f, "face:{}", id.0),
            Instance::Array(id) => write!(f, "array:{}", id.0),
            Instance::Index(i) => write!(f, "#{i}"),
        }
    }
}

/// A node of the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct NodeKey {
    pub kind: KindId,
    pub instance: Instance,
}

impl NodeKey {
    #[must_use]
    pub const fn new(kind: KindId, instance: Instance) -> Self {
        Self { kind, instance }
    }
}

impl fmt::Display for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{}]", self.kind, self.instance)
    }
}
