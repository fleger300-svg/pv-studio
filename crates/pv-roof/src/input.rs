//! From a building to the generator's input: the roof outline (footprint moved out by the
//! overhang) and one plane per edge, in the building frame (east, north, up from `origin`).
//!
//! The plane of edge `e` (inward unit normal `n_e`, a point `a_e` of its outline edge, pitch
//! `θ_e`, eave height `h_e`) is `z_e(p) = h_e + tan θ_e · n_e·(p − a_e)`. With an overhang
//! `o`, the outline edge lies `o` outside the wall and the eave drops by `o · tan θ_e`: the
//! plane itself does not move.

use pv_geo::{offset_mitered, outward_normal, Polygon, Vec2};
use pv_model::roof::{validate_footprint, validate_roof_spec};
use pv_model::Building;

use crate::RoofError;

/// Plane of one footprint edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePlane {
    /// Index of the footprint edge.
    pub edge: usize,
    /// Pitch, degrees (90: vertical, the edge carries no roof plane).
    pub pitch_deg: f64,
    /// Inward unit normal of the outline edge (the up-slope direction in plan).
    pub inward: Vec2,
    /// Start vertex of the outline edge.
    pub anchor: Vec2,
    /// `tan θ` (meaningless when vertical).
    pub tan: f64,
    /// Height of the outline edge (the eave, lowered by the overhang), m.
    pub base: f64,
}

impl EdgePlane {
    #[must_use]
    pub fn is_sloped(&self) -> bool {
        self.pitch_deg < 90.0
    }

    /// Height of the plane above `p` (building frame).
    #[must_use]
    pub fn z(&self, p: Vec2) -> f64 {
        self.base + self.tan * self.inward.dot(p - self.anchor)
    }
}

/// The generator's input.
#[derive(Debug, Clone, PartialEq)]
pub struct RoofInput {
    /// Wall footprint, counter-clockwise, building frame.
    pub footprint: Vec<Vec2>,
    /// Roof outline (footprint moved out by the overhang); edge `i` ↔ footprint edge `i`.
    pub outline: Vec<Vec2>,
    /// One per edge.
    pub planes: Vec<EdgePlane>,
    /// The building's eave height (flat roofs), m.
    pub eave_m: f64,
}

impl RoofInput {
    /// Validates the building and builds the planes.
    ///
    /// # Errors
    /// [`RoofError::NoSpec`], [`RoofError::Invalid`] or [`RoofError::Overhang`].
    pub fn new(b: &Building) -> Result<Self, RoofError> {
        let spec = b.roof.as_ref().ok_or(RoofError::NoSpec)?;
        validate_footprint(&b.footprint).map_err(|e| RoofError::Invalid(e.field()))?;
        validate_roof_spec(spec, &b.footprint).map_err(|e| RoofError::Invalid(e.field()))?;
        let footprint: Vec<Vec2> = b.footprint.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        let n = footprint.len();
        let outline = if spec.overhang_m > 0.0 {
            let o = offset_mitered(&footprint, spec.overhang_m);
            match Polygon::new(o.clone()) {
                Ok(p) if p.len() == n => o,
                _ => return Err(RoofError::Overhang),
            }
        } else {
            footprint.clone()
        };
        let planes = (0..n)
            .map(|i| {
                let e = spec.edges[i];
                let (a, c) = (outline[i], outline[(i + 1) % n]);
                let tan = if e.is_sloped() {
                    e.pitch_deg.to_radians().tan()
                } else {
                    f64::INFINITY
                };
                let eave = e.eave_height_m.unwrap_or(b.eave_height_m);
                EdgePlane {
                    edge: i,
                    pitch_deg: e.pitch_deg,
                    inward: -outward_normal(a, c),
                    anchor: a,
                    tan,
                    base: if e.is_sloped() {
                        eave - spec.overhang_m * tan
                    } else {
                        eave
                    },
                }
            })
            .collect();
        Ok(Self {
            footprint,
            outline,
            planes,
            eave_m: b.eave_height_m,
        })
    }

    /// Indices of the sloped edges.
    pub fn sloped(&self) -> impl Iterator<Item = usize> + '_ {
        self.planes.iter().filter(|p| p.is_sloped()).map(|p| p.edge)
    }
}
