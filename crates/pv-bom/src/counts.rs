//! What the bill of materials counts on a laid-out field. Built by `pv-pipeline` from the
//! layout; `pv-bom` never looks at the geometry itself.

use pv_model::PartRule;
use serde::Serialize;

/// A roof face (one tab of the bill of materials).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FaceRef {
    /// Stable identity (grouping key).
    pub id: String,
    /// Tab title.
    pub name: String,
}

/// A top rail and the modules it carries.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TopRail {
    pub length_m: f64,
    pub modules: u32,
}

/// Counters of one field, and the parts of its mounting system.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldCounts {
    pub face: FaceRef,
    /// Active modules.
    pub modules: u32,
    pub top_rails: Vec<TopRail>,
    /// Modules carried by each module-carrying rail: the top rails of a two-layer system,
    /// the rails of a one-layer system. End and middle clamps are counted on them.
    pub clamp_rails: Vec<u32>,
    /// Bottom rail pieces [m].
    pub bottom_rails: Vec<f64>,
    /// Crossings of top and bottom rails.
    pub climbers: u32,
    /// Roof fixations.
    pub fixations: u32,
    pub ballast_blocks: u32,
    pub parts: Vec<PartRule>,
}
