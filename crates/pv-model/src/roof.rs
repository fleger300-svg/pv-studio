//! Roof specification of a building: what the roof generator (`pv-roof`) turns into roof
//! faces. One entry per footprint edge: its pitch (90° = vertical gable wall, the edge
//! carries no roof plane) and, optionally, a raised eave (half-hipped ends).

use serde::{Deserialize, Serialize};
use specta::Type;

/// Roof data of one footprint edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRoof {
    /// Pitch of the roof plane rising from this edge, degrees in `]0, 90]`; 90 = gable wall.
    pub pitch_deg: f64,
    /// Height of this edge's eave above the ground, m; `None`: the building's eave height.
    /// Higher than the building's eave on a half-hipped end.
    #[specta(optional)]
    pub eave_height_m: Option<f64>,
}

impl EdgeRoof {
    /// A sloped edge at the building's eave height.
    #[must_use]
    pub fn sloped(pitch_deg: f64) -> Self {
        Self {
            pitch_deg,
            eave_height_m: None,
        }
    }

    /// A vertical gable wall.
    #[must_use]
    pub fn gable() -> Self {
        Self::sloped(90.0)
    }

    /// Whether the edge carries a roof plane (pitch below 90°).
    #[must_use]
    pub fn is_sloped(&self) -> bool {
        self.pitch_deg < 90.0
    }
}

/// How the roof of a building is generated from its footprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofSpec {
    /// One per footprint edge (edge `i` joins footprint vertices `i` and `i + 1`). All edges
    /// vertical: a flat roof.
    pub edges: Vec<EdgeRoof>,
    /// Horizontal overhang of the roof beyond the walls, m (same on every edge).
    pub overhang_m: f64,
}

/// Axis-aligned rectangular footprint centred on the building origin, counter-clockwise from
/// the south-west corner.
#[must_use]
pub fn rectangle_footprint(size_m: [f64; 2]) -> Vec<[f64; 2]> {
    let (hx, hy) = (size_m[0] / 2.0, size_m[1] / 2.0);
    vec![[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]]
}

/// Why a footprint or a roof specification is invalid (`errors.entity.invalid` field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofSpecError {
    Footprint,
    Edges,
    Pitch,
    EaveHeight,
    Overhang,
}

impl RoofSpecError {
    /// The field name reported to the user.
    #[must_use]
    pub fn field(self) -> &'static str {
        match self {
            RoofSpecError::Footprint => "footprint",
            RoofSpecError::Edges => "roofEdges",
            RoofSpecError::Pitch => "pitchDeg",
            RoofSpecError::EaveHeight => "eaveHeightM",
            RoofSpecError::Overhang => "overhangM",
        }
    }
}

/// Shortest footprint edge accepted, m.
pub const MIN_FOOTPRINT_EDGE_M: f64 = 0.05;

/// Checks a footprint: at least 3 vertices, finite, counter-clockwise, simple, edges of at
/// least [`MIN_FOOTPRINT_EDGE_M`].
///
/// # Errors
/// [`RoofSpecError::Footprint`].
pub fn validate_footprint(footprint: &[[f64; 2]]) -> Result<(), RoofSpecError> {
    let n = footprint.len();
    if n < 3 || footprint.iter().flatten().any(|c| !c.is_finite()) {
        return Err(RoofSpecError::Footprint);
    }
    for i in 0..n {
        let (a, b) = (footprint[i], footprint[(i + 1) % n]);
        if (b[0] - a[0]).hypot(b[1] - a[1]) < MIN_FOOTPRINT_EDGE_M {
            return Err(RoofSpecError::Footprint);
        }
    }
    let pts: Vec<pv_geo::Vec2> = footprint
        .iter()
        .map(|p| pv_geo::Vec2::new(p[0], p[1]))
        .collect();
    let twice_area: f64 = (0..n).map(|i| pts[i].cross(pts[(i + 1) % n])).sum();
    if twice_area <= 0.0 {
        return Err(RoofSpecError::Footprint);
    }
    match pv_geo::Polygon::new(pts) {
        Ok(p) if p.len() == n => Ok(()),
        _ => Err(RoofSpecError::Footprint),
    }
}

/// Checks a roof specification against its footprint (one entry per edge, pitches in
/// `]0, 90]`, finite non-negative heights and overhang).
///
/// # Errors
/// The first invalid field.
pub fn validate_roof_spec(spec: &RoofSpec, footprint: &[[f64; 2]]) -> Result<(), RoofSpecError> {
    if spec.edges.len() != footprint.len() {
        return Err(RoofSpecError::Edges);
    }
    for e in &spec.edges {
        if !(e.pitch_deg.is_finite() && e.pitch_deg > 0.0 && e.pitch_deg <= 90.0) {
            return Err(RoofSpecError::Pitch);
        }
        if e.eave_height_m.is_some_and(|h| !h.is_finite() || h < 0.0) {
            return Err(RoofSpecError::EaveHeight);
        }
    }
    if !spec.overhang_m.is_finite() || !(0.0..=5.0).contains(&spec.overhang_m) {
        return Err(RoofSpecError::Overhang);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_rules() {
        assert_eq!(
            validate_footprint(&rectangle_footprint([10.0, 6.0])),
            Ok(())
        );
        // Clockwise.
        let mut cw = rectangle_footprint([10.0, 6.0]);
        cw.reverse();
        assert!(validate_footprint(&cw).is_err());
        // Bow tie.
        let bow = [[0.0, 0.0], [4.0, 4.0], [4.0, 0.0], [0.0, 4.0]];
        assert!(validate_footprint(&bow).is_err());
        // Tiny edge.
        let tiny = [[0.0, 0.0], [4.0, 0.0], [4.0, 0.01], [4.0, 4.0], [0.0, 4.0]];
        assert!(validate_footprint(&tiny).is_err());
        assert!(validate_footprint(&[[0.0, 0.0], [1.0, 0.0]]).is_err());
    }

    #[test]
    fn roof_spec_rules() {
        let fp = rectangle_footprint([10.0, 6.0]);
        let ok = RoofSpec {
            edges: vec![
                EdgeRoof::sloped(30.0),
                EdgeRoof::gable(),
                EdgeRoof::sloped(30.0),
                EdgeRoof::gable(),
            ],
            overhang_m: 0.5,
        };
        assert_eq!(validate_roof_spec(&ok, &fp), Ok(()));
        let mut bad = ok.clone();
        bad.edges.pop();
        assert_eq!(validate_roof_spec(&bad, &fp), Err(RoofSpecError::Edges));
        let mut bad = ok.clone();
        bad.edges[0].pitch_deg = 0.0;
        assert_eq!(validate_roof_spec(&bad, &fp), Err(RoofSpecError::Pitch));
        let mut bad = ok;
        bad.overhang_m = -0.1;
        assert_eq!(validate_roof_spec(&bad, &fp), Err(RoofSpecError::Overhang));
    }
}
