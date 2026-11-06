//! Layout data stored with roof faces and arrays (consumed by `pv-layout`).
//!
//! All coordinates are in the roof face's `(u, v)` frame, metres. A face without
//! [`RoofOutlineSpec`] is its `width_m × height_m` rectangle; an array with default
//! [`ArrayLayoutParams`] covers the whole face with no setback. Projects saved before these
//! fields existed load with those defaults (`pv-store` fills them in). The types carry no
//! `#[serde(default)]` so that the TypeScript bindings keep a single shape per type.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Kind of a roof edge (setbacks and wind zones depend on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RoofEdgeKind {
    /// Lower horizontal edge (gutter).
    Eave,
    /// Upper horizontal edge where two faces meet.
    Ridge,
    /// Sloped free edge (gable).
    Verge,
    /// Sloped edge shared with an adjacent face (hipped roofs).
    Hip,
    /// Internal edge between two faces draining towards each other.
    Valley,
    /// Edge of a flat roof bordered by a parapet.
    Parapet,
    /// Edge against a rising wall.
    Abutment,
    Other,
}

/// Kind of an obstacle on the roof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ObstacleKind {
    Chimney,
    Skylight,
    Vent,
    Dormer,
    /// Area kept free on purpose (access path, maintenance zone…).
    KeepOut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofObstacle {
    pub name: String,
    pub kind: ObstacleKind,
    /// Footprint polygon on the roof plane.
    pub footprint_uv: Vec<[f64; 2]>,
    /// Height above the roof surface, m.
    pub height_m: f64,
    /// Clearance overriding the rule for this obstacle kind, m.
    #[specta(optional)]
    pub clearance_m: Option<f64>,
}

/// Outline of a roof face that is not a plain rectangle, with typed edges and obstacles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofOutlineSpec {
    /// Polygon vertices; edge `i` joins vertices `i` and `i + 1`.
    pub vertices_uv: Vec<[f64; 2]>,
    /// One kind per edge.
    pub edge_kinds: Vec<RoofEdgeKind>,
    pub obstacles: Vec<RoofObstacle>,
}

/// Grouping of modules into mounting blocks (rails continuous inside a block only).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BlockParams {
    /// Maximum modules per block along `u` / `v`.
    pub max_cols: u32,
    pub max_rows: u32,
    /// Clear distance between the modules of two adjacent blocks along `u` / `v`, m.
    pub gap_u_m: f64,
    pub gap_v_m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EdgeSetback {
    pub kind: RoofEdgeKind,
    pub distance_m: f64,
}

/// Where the grid is anchored when the engine searches the best offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LayoutAnchor {
    /// Lower-left corner (eave, left verge): K2 Base convention.
    #[default]
    LowerLeft,
    LowerRight,
    UpperLeft,
    UpperRight,
    Centre,
}

/// Kind of support grid under the roof covering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SupportGridKind {
    /// Standing seams: one clamp on every seam under a rail.
    StandingSeam,
    /// Rafters or purlins (tiles, corrugated sheets): fixations every n-th line.
    Rafters,
}

/// Seams or rafters of a roof face: lines `u = origin + k·spacing` (they run up the slope).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupportGridSpec {
    pub kind: SupportGridKind,
    /// `u` of one seam / rafter, m.
    pub origin_m: f64,
    pub spacing_m: f64,
}

/// Geometry rules of a cross-rail mounting system (top rails on bottom rails), from the
/// mounting system's data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CrossRailSpec {
    pub per_column: u32,
    pub top_inset_m: f64,
    pub top_overhang_m: f64,
    pub bottom_spacing_m: f64,
    pub min_support_overhang_m: f64,
    pub spacing_step_m: f64,
    pub bottom_inset_m: f64,
    /// Fixations keep this distance from the bottom-rail ends, m.
    pub fixation_edge_m: f64,
    /// Maximum bottom-rail cantilever beyond the outer fixation, m.
    #[specta(optional)]
    pub max_cantilever_m: Option<f64>,
}

/// Direction of an expansion joint line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum JointAxis {
    /// Line `u = position`: cuts the rails running along `u`.
    ConstantU,
    /// Line `v = position`: cuts the rails running along `v`.
    ConstantV,
}

/// Expansion joint placed by the user (the architecture's `expansion_joint`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpansionJointSpec {
    pub axis: JointAxis,
    /// Roof coordinate of the line, m.
    pub position_m: f64,
}

/// Layout parameters of an array (the architecture's `params_json` / `boundary_json`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArrayLayoutParams {
    /// Field outline on the face; empty = the whole face.
    pub boundary_uv: Vec<[f64; 2]>,
    /// Clear distance between adjacent modules along `u` / `v` inside a block, m.
    pub module_gap_u_m: f64,
    pub module_gap_v_m: f64,
    #[specta(optional)]
    pub block: Option<BlockParams>,
    /// Setback for edges not listed in `edge_setbacks`, m.
    pub edge_setback_m: f64,
    pub edge_setbacks: Vec<EdgeSetback>,
    /// Clearance around obstacles (unless the obstacle overrides it), m.
    pub obstacle_clearance_m: f64,
    /// Row pitch along `v` for tilted rows, or pair pitch along `u` for east-west systems, m.
    #[specta(optional)]
    pub row_pitch_m: Option<f64>,
    pub anchor: LayoutAnchor,
    /// Expansion joints placed by the user.
    pub expansion_joints: Vec<ExpansionJointSpec>,
    /// Maximum length of a mounting block along `u` / `v` (continuous rails, from the
    /// mounting system's thermal rule), m. `None` = no automatic joint in that direction.
    #[specta(optional)]
    pub max_block_length_u_m: Option<f64>,
    #[specta(optional)]
    pub max_block_length_v_m: Option<f64>,
    /// Minimum clear distance between this array's modules and the modules of the other
    /// arrays of the face, m.
    pub min_distance_to_other_arrays_m: f64,
    /// Rail geometry of the mounting system (cross-rail systems).
    #[specta(optional)]
    pub rails: Option<CrossRailSpec>,
    /// The other half of an east-west system (the +90° / −90° partner array): the two
    /// halves are one mounting system, the distance between arrays does not apply to them.
    #[specta(optional)]
    pub paired_with: Option<crate::ids::ArrayId>,
}
