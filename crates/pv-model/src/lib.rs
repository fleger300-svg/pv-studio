//! PV Studio domain model.
//!
//! Conventions (see also [`units`]):
//! - SI units everywhere (metres). Angles are stored in degrees, as documented on each field,
//!   and converted to radians only inside computations.
//! - World frame: local ENU (x = east, y = north, z = up), centred on the project origin.
//! - Compass azimuth: 0° = north, 90° = east, 180° = south, 270° = west.
//!
//! This crate has no dependency on Tauri or on storage.

pub mod command;
pub mod geometry;
pub mod ids;
pub mod layout;
pub mod project;
pub mod stress;
pub mod summary;
pub mod units;

pub use command::{ChangeSet, Command, CommandError, ErrorParam};
pub use geometry::{Quat, RoofFrame, Vec3};
pub use ids::{ArrayId, BuildingId, ModuleId, ProjectId, RoofFaceId};
pub use layout::{
    ArrayLayoutParams, BlockParams, EdgeSetback, LayoutAnchor, ObstacleKind, RoofEdgeKind,
    RoofObstacle, RoofOutlineSpec,
};
pub use project::{
    ArrayGrid, Building, ModuleInstance, ModuleState, Orientation, PanelArray, Project, RoofFace,
    RoofKind, MODULES_PER_STRING, MODULE_MOUNT_GAP_M, MODULE_THICKNESS_M,
};
pub use stress::StressPreset;
pub use summary::{ArraySummary, ProjectSummary, RoofFaceSummary};
