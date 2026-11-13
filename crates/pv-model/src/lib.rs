//! PV Studio domain model.
//!
//! Conventions (see also [`units`]):
//! - SI units everywhere (metres). Angles are stored in degrees, as documented on each field,
//!   and converted to radians only inside computations.
//! - World frame: local ENU (x = east, y = north, z = up), centred on the project origin.
//! - Compass azimuth: 0° = north, 90° = east, 180° = south, 270° = west.
//!
//! This crate has no dependency on Tauri or on storage.

pub mod article;
pub mod command;
pub mod component;
pub mod fingerprint;
pub mod geometry;
pub mod ids;
pub mod layout;
pub mod mounting;
pub mod pricing;
pub mod project;
pub mod roof;
pub mod site;
pub mod stress;
pub mod summary;
pub mod units;

pub use article::Article;
pub use command::{ChangeSet, Command, CommandError, ErrorParam, ModulePosition, ModuleSlot};
pub use component::{Component, ComponentSnapshot, ComponentSpec};
pub use fingerprint::fingerprint;
pub use geometry::{Quat, RoofFrame, Vec3};
pub use ids::{ArrayId, BlobId, BuildingId, ComponentId, ModuleId, ProjectId, RoofFaceId};
pub use layout::{
    ArrayLayoutParams, BlockParams, CrossRailSpec, EdgeSetback, ExpansionJointSpec, JointAxis,
    LayoutAnchor, ObstacleKind, RailLayers, RoofEdgeKind, RoofObstacle, RoofOutlineSpec,
    SupportGridKind, SupportGridSpec,
};
pub use mounting::{
    AeroZoneSpec, BallastSpec, CpEntrySpec, CpSpec, Driver, FixationSpec, MembraneKind,
    MountingSystem, PairCoefficientsSpec, PartRule, RailLayerKind, RailSectionSpec, RailStatics,
    WindZone,
};
pub use pricing::{ManufacturerDiscount, Pricing};
pub use project::{
    ArrayGrid, Building, ModuleInstance, ModuleState, Orientation, PanelArray, Project, RoofFace,
    RoofKind, MODULES_PER_STRING, MODULE_MOUNT_GAP_M, MODULE_THICKNESS_M,
};
pub use roof::{rectangle_footprint, EdgeRoof, RoofSpec};
pub use site::{ConsequenceClass, ServiceLifeModel, SiteParams, SnowExposure, TerrainCategory};
pub use stress::StressPreset;
pub use summary::{ArraySummary, ProjectSummary, RoofFaceSummary, WallSummary};
