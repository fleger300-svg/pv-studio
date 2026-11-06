//! PV Studio layout engine (calepinage).
//!
//! Works in the `(u, v)` frame of one roof face (`pv-model` convention: `u` along the eave,
//! `v` up the slope, origin at the left end of the eave). Its outputs are neutral geometry —
//! modules, mounting blocks, rails, supports, links — consumed by `pv-statics` (load descent,
//! wind-zone assignment, ballast sharing) and `pv-bom` (cuts and quantities).
//!
//! Milestones:
//! - L1 [`roof`], [`setbacks`]: roof outline with typed edges and obstacles, setbacks,
//!   acceptance test of a module footprint.
//! - L2 grid generation, L3 blocks and expansion joints, L4 rails and supports, L5 mixing of
//!   systems on one face and command validation (to come).

pub mod roof;
pub mod setbacks;

pub use roof::{EdgeKind, Obstacle, ObstacleKind, RoofOutline};
pub use setbacks::{BuildableArea, Rejection, SetbackRules};

/// Errors of the layout engine.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LayoutError {
    #[error("invalid roof outline: {0}")]
    Outline(pv_geo::PolygonError),
    #[error("{kinds} edge kinds given for {vertices} vertices")]
    EdgeKindCount { vertices: usize, kinds: usize },
    #[error("obstacle {name} is not on the roof")]
    ObstacleOffRoof { name: String },
}
