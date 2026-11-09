//! PV Studio roof generator.
//!
//! A building's footprint and [`RoofSpec`](pv_model::RoofSpec) (a pitch per edge, 90° for a
//! gable wall, raised eaves for half-hipped ends, an overhang) become roof faces:
//!
//! - convex footprints: the lower envelope of the edge planes ([`envelope`]);
//! - concave footprints (L, T, U): the weighted straight skeleton (`pv_geo::skeleton`),
//!   which coincides with the envelope on convex ones — the tests use one as the other's
//!   oracle.
//!
//! [`plan`] turns the result into an explicit `Command::Batch` (building, faces upserted,
//! faces removed); the journal records that batch, never "regenerate", so a later version
//! of this algorithm cannot change an existing history. [`walls`] derives the walls drawn
//! under any roof.

pub mod classify;
pub mod envelope;
pub mod faces;
pub mod input;
pub mod plan;
pub mod presets;
pub mod walls;

use pv_geo::{is_convex, min_area_rect, Vec2};
use pv_model::{Building, RoofEdgeKind, RoofFace, RoofKind};

pub use input::{EdgePlane, RoofInput};
pub use plan::plan_roof;
pub use walls::walls;

/// Why a roof cannot be generated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoofError {
    #[error("the building has no roof specification")]
    NoSpec,
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("the overhang makes the roof outline self-intersect")]
    Overhang,
    #[error("the footprint is not convex")]
    NonConvex,
    #[error("the roof is degenerate")]
    Degenerate,
}

/// A face in plan: a counter-clockwise polygon in the building frame lying on the plane of
/// footprint edge `edge` (`None`: the flat roof at the eave height).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanFace {
    pub edge: Option<usize>,
    pub polygon: Vec<Vec2>,
}

/// Drops repeated and collinear vertices of a polygon.
#[must_use]
pub fn clean(mut poly: Vec<Vec2>) -> Vec<Vec2> {
    loop {
        let n = poly.len();
        if n < 3 {
            return poly;
        }
        let drop = (0..n).find(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            a.distance(b) <= 1e-9 || {
                let (ab, bc) = (b - a, c - b);
                ab.cross(bc).abs() <= 1e-12 * ab.length() * bc.length() && ab.dot(bc) > 0.0
            }
        });
        match drop {
            Some(i) => {
                poly.remove(i);
            }
            None => return poly,
        }
    }
}

/// A generated roof.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedRoof {
    pub input: RoofInput,
    pub plan_faces: Vec<PlanFace>,
    /// Faces as `RoofFace`s (nil ids, compass names), in plan-face order.
    pub faces: Vec<RoofFace>,
    pub roof_kind: RoofKind,
    /// Highest point of the roof above the ground, m.
    pub ridge_height_m: f64,
    /// Minimum-area enclosing rectangle of the footprint, long side first, m.
    pub size_m: [f64; 2],
}

impl GeneratedRoof {
    /// The building with its derived fields (`roof_kind`, `ridge_height_m`, `size_m`) set.
    #[must_use]
    pub fn building(&self, b: &Building) -> Building {
        Building {
            roof_kind: self.roof_kind,
            ridge_height_m: self.ridge_height_m,
            size_m: self.size_m,
            ..b.clone()
        }
    }
}

/// Faces in plan for the input: flat, envelope (convex) or skeleton (concave).
///
/// # Errors
/// [`RoofError::NonConvex`] when a concave outline needs the skeleton and cannot get it.
pub fn plan_faces(input: &RoofInput) -> Result<Vec<PlanFace>, RoofError> {
    if input.sloped().next().is_none() {
        return Ok(vec![PlanFace {
            edge: None,
            polygon: input.outline.clone(),
        }]);
    }
    if is_convex(&input.outline) {
        Ok(envelope::envelope_faces(input))
    } else {
        Err(RoofError::NonConvex)
    }
}

/// Generates the roof of a building with a roof specification.
///
/// # Errors
/// See [`RoofError`].
pub fn generate(b: &Building) -> Result<GeneratedRoof, RoofError> {
    let input = RoofInput::new(b)?;
    let plan_faces = plan_faces(&input)?;
    if plan_faces.is_empty() {
        return Err(RoofError::Degenerate);
    }
    let kinds = classify::edge_kinds(&input, &plan_faces);
    let roof_kind = classify::roof_kind(&plan_faces, &kinds);
    let faces: Vec<RoofFace> = plan_faces
        .iter()
        .zip(&kinds)
        .map(|(pf, k)| {
            let mut f = faces::roof_face(b, &input, pf, k);
            f.name = faces::compass_name(f.tilt_deg, f.azimuth_deg);
            f
        })
        .collect();
    let ridge = plan_faces
        .iter()
        .flat_map(|pf| {
            let input = &input;
            pf.polygon
                .iter()
                .map(move |&p| classify::face_z(input, pf, p))
        })
        .fold(f64::NEG_INFINITY, f64::max)
        .max(b.eave_height_m);
    let rect = min_area_rect(&input.footprint).ok_or(RoofError::Degenerate)?;
    Ok(GeneratedRoof {
        faces,
        roof_kind,
        ridge_height_m: ridge,
        size_m: [2.0 * rect.half_u, 2.0 * rect.half_v],
        plan_faces,
        input,
    })
}

/// Kinds that make an edge an outline edge of the roof (eave, verge, parapet).
#[must_use]
pub fn is_outline_kind(k: RoofEdgeKind) -> bool {
    matches!(
        k,
        RoofEdgeKind::Eave | RoofEdgeKind::Verge | RoofEdgeKind::Parapet
    )
}

impl From<RoofError> for pv_model::CommandError {
    fn from(e: RoofError) -> Self {
        use pv_model::command::keys;
        let field = match e {
            RoofError::NoSpec => "roof",
            RoofError::Invalid(field) => field,
            RoofError::Overhang => "overhangM",
            RoofError::NonConvex | RoofError::Degenerate => "footprint",
        };
        pv_model::CommandError::new(keys::ENTITY_INVALID).with_text("field", field)
    }
}
