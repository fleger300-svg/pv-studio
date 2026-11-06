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
}
