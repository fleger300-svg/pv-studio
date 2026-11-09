//! Derived nodes of the domain graph (see the table in the crate documentation).

use pv_graph::{GraphError, Instance};
use pv_model::command::keys;
use pv_model::{ArrayId, BuildingId, CommandError, RoofFaceId};

pub mod ballast;
pub mod bom;
pub mod descent;
pub mod geometry;
pub mod layout;
pub mod loads;
pub mod summary;
pub mod zones;

/// Declares a node kind whose value is fingerprinted through serde.
macro_rules! kind {
    ($(#[$doc:meta])* $name:ident, $id:literal, $ty:ty) => {
        $(#[$doc])*
        pub struct $name;
        impl pv_graph::Kind for $name {
            type Output = $ty;
            const NAME: &'static str = $id;
            fn fingerprint(v: &$ty) -> u64 {
                pv_graph::serde_fingerprint(v)
            }
        }
    };
}
pub(crate) use kind;

fn wrong_instance(expected: &str) -> GraphError {
    CommandError::new(keys::LAYOUT_INVALID_PARAMETERS)
        .with_text("detail", format!("node expects a {expected} instance"))
        .into()
}

pub(crate) fn array_of(i: Instance) -> Result<ArrayId, GraphError> {
    match i {
        Instance::Array(a) => Ok(a),
        _ => Err(wrong_instance("array")),
    }
}

pub(crate) fn face_of(i: Instance) -> Result<RoofFaceId, GraphError> {
    match i {
        Instance::Face(f) => Ok(f),
        _ => Err(wrong_instance("face")),
    }
}

pub(crate) fn building_of(i: Instance) -> Result<BuildingId, GraphError> {
    match i {
        Instance::Building(b) => Ok(b),
        _ => Err(wrong_instance("building")),
    }
}
