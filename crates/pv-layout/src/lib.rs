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
//! - L2 [`grid`]: lattice with mounting blocks, fixed placement, best-offset filling,
//!   coplanar / tilted rows / east-west pairs; [`model`]: bridge with `pv-model`.
//! - L3 [`joints`], [`blocks`]: expansion joints (manual and thermal rule), mounting
//!   blocks, mechanical links for ballast sharing.
//! - L4 [`supports`], [`rails`]: support grids (standing seams, rafters) and their phase,
//!   cross-rail tracing (top / bottom rails, climbers), fixations on the support lines.
//! - L5 [`mixing`]: several arrays / mounting systems on one face; [`validate`]: command
//!   validation (setbacks, obstacles, collisions, distance between arrays); rails and
//!   support grid read from the stored parameters ([`model::hardware`]).

pub mod blocks;
pub mod grid;
pub mod joints;
pub mod mixing;
pub mod model;
pub mod rails;
pub mod roof;
pub mod setbacks;
pub mod supports;
pub mod validate;

pub use blocks::{build_structure, CellLink, Joint, MountingBlock, Structure};
pub use grid::{
    fill, fill_east_west, place_fixed, Cell, EastWestPlacement, EastWestSpec, GridSpec, Placement,
    Region, Search,
};
pub use joints::{JointRules, JointSource};
pub use mixing::{layout_face, FieldLayout, FieldPlan};
pub use model::{layout_array, layout_east_west, relayout_fixed, ArrayLayout};
pub use rails::{
    cross_rails, fixations, CrossRailParams, Fixations, Rail, RailLayer, RailLayout, TopSupports,
};
pub use roof::{EdgeKind, Obstacle, ObstacleKind, RoofOutline};
pub use setbacks::{BuildableArea, Rejection, SetbackRules};
pub use supports::{SupportKind, SupportLines};
pub use validate::{face_conflicts, validate_module_positions};

/// Errors of the layout engine.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LayoutError {
    #[error("invalid roof outline: {0}")]
    Outline(pv_geo::PolygonError),
    #[error("{kinds} edge kinds given for {vertices} vertices")]
    EdgeKindCount { vertices: usize, kinds: usize },
    #[error("obstacle {name} is not on the roof")]
    ObstacleOffRoof { name: String },
    #[error("the array does not belong to this roof face")]
    WrongFace,
    #[error("an east-west system needs one array at +90° and one at −90° azimuth offset")]
    NotEastWest,
    #[error("an east-west system needs its pair pitch (row_pitch_m)")]
    MissingPairPitch,
    #[error("the {axis} pitch {pitch_m} m is smaller than the footprint {footprint_m} m: modules would overlap")]
    PitchTooSmall {
        axis: &'static str,
        pitch_m: f64,
        footprint_m: f64,
    },
    #[error("a {module_m} m module is longer than the maximum block length {max_m} m ({axis:?})")]
    JointRuleUnsatisfiable {
        axis: pv_model::JointAxis,
        module_m: f64,
        max_m: f64,
    },
}
