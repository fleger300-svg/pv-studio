//! Input nodes: the project, cut finely so that a change invalidates only what reads it.
//!
//! | Input | Instance | Value |
//! |---|---|---|
//! | [`BuildingIn`] | building | geometry and roof kind (site stripped) |
//! | [`SiteIn`] | building | site data for the statics |
//! | [`FaceIn`] | face | outline, pitch, support grid |
//! | [`ArrayIn`] | array | module, grid and layout parameters (mounting stripped) |
//! | [`MountingIn`] | array | statics snapshot of the mounting system |
//! | [`ModulesIn`] | array | the array's module instances |
//! | [`ProjectIndexIn`] | project | ids of every entity (to forget deleted ones) |

use pv_graph::{serde_fingerprint, Input, Instance, Kind};
use pv_model::fingerprint::fingerprint_bytes;
use pv_model::{
    ArrayId, Building, BuildingId, ModuleInstance, ModuleState, MountingSystem, PanelArray,
    RoofFace, RoofFaceId, SiteParams,
};
use serde::Serialize;

macro_rules! serde_input {
    ($(#[$doc:meta])* $name:ident, $id:literal, $ty:ty) => {
        $(#[$doc])*
        pub struct $name;
        impl Kind for $name {
            type Output = $ty;
            const NAME: &'static str = $id;
            fn fingerprint(v: &$ty) -> u64 {
                serde_fingerprint(v)
            }
        }
        impl Input for $name {}
    };
}

serde_input!(
    /// A building without its site data.
    BuildingIn,
    "in.building",
    Building
);
serde_input!(
    /// Site data of a building.
    SiteIn,
    "in.site",
    Option<SiteParams>
);
serde_input!(
    /// A roof face.
    FaceIn,
    "in.face",
    RoofFace
);
serde_input!(
    /// An array without its mounting snapshot.
    ArrayIn,
    "in.array",
    PanelArray
);
serde_input!(
    /// Mounting snapshot of an array.
    MountingIn,
    "in.mounting",
    Option<MountingSystem>
);
serde_input!(
    /// Ids of every entity of the project.
    ProjectIndexIn,
    "in.index",
    ProjectIndex
);

/// Module instances of one array.
pub struct ModulesIn;
impl Kind for ModulesIn {
    type Output = Vec<ModuleInstance>;
    const NAME: &'static str = "in.modules";
    fn fingerprint(v: &Vec<ModuleInstance>) -> u64 {
        modules_fingerprint(v)
    }
}
impl Input for ModulesIn {}

/// Exact binary fingerprint of module instances (every field, f64 by bit pattern): the
/// JSON form of 1 000 modules costs milliseconds, this costs microseconds.
#[must_use]
pub fn modules_fingerprint(modules: &[ModuleInstance]) -> u64 {
    let mut bytes = Vec::with_capacity(modules.len() * 49);
    for m in modules {
        bytes.extend_from_slice(&m.id.0.to_le_bytes());
        bytes.extend_from_slice(m.array_id.0.as_bytes());
        bytes.extend_from_slice(&m.row.to_le_bytes());
        bytes.extend_from_slice(&m.col.to_le_bytes());
        bytes.extend_from_slice(&m.u.to_bits().to_le_bytes());
        bytes.extend_from_slice(&m.v.to_bits().to_le_bytes());
        bytes.push(match m.state {
            ModuleState::Active => 1,
            ModuleState::Removed => 0,
        });
        bytes.extend_from_slice(&m.string_index.to_le_bytes());
    }
    fingerprint_bytes(&bytes)
}

/// Ids of the project's entities and their parents.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProjectIndex {
    pub buildings: Vec<BuildingId>,
    pub faces: Vec<(RoofFaceId, BuildingId)>,
    pub arrays: Vec<(ArrayId, RoofFaceId)>,
}

impl ProjectIndex {
    /// Every instance of the index.
    #[must_use]
    pub fn instances(&self) -> Vec<Instance> {
        self.buildings
            .iter()
            .map(|&b| Instance::Building(b))
            .chain(self.faces.iter().map(|&(f, _)| Instance::Face(f)))
            .chain(self.arrays.iter().map(|&(a, _)| Instance::Array(a)))
            .collect()
    }
}
