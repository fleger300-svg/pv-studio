//! PV Studio roof generator.
//!
//! A building's footprint and [`RoofSpec`](pv_model::RoofSpec) (a pitch per edge, 90° for a
//! gable wall, raised eaves for half-hipped ends, an overhang) become roof faces:
//!
//! The roof is the *straight-skeleton roof*: every face rises from its own eave, at its
//! pitch, until it meets the others (time = height in the weighted skeleton,
//! `pv_geo::weighted_skeleton`). On a convex footprint it is the lower envelope of the edge
//! planes ([`envelope`]) — exact, one convex clip per pair of edges — except when a plane
//! of low pitch would come back as a face detached from its eave (draining through a valley
//! into a neighbour): the envelope is used when every one of its faces touches its eave, the
//! skeleton otherwise. The tests use each as the other's oracle.
//!
//! [`plan`] turns the result into an explicit `Command::Batch` (building, faces upserted,
//! faces removed); the journal records that batch, never "regenerate", so a later version
//! of this algorithm cannot change an existing history. [`walls`] derives the walls drawn
//! under any roof.

pub mod classify;
pub mod envelope;
pub mod faces;
pub mod form;
pub mod input;
pub mod plan;
pub mod presets;
pub mod walls;

use pv_geo::{is_convex, min_area_rect, signed_area, weighted_skeleton, Vec2, WaveEdge};
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
    #[error("the roof is degenerate")]
    Degenerate,
    /// At a re-entrant corner of a concave footprint, edges `a` and `b` (footprint edge
    /// indices) must both be sloped and start at the same height: a gable wall, a raised
    /// eave or (with an overhang) unequal pitches there would put a vertical step inside the
    /// roof.
    #[error("edges {a} and {b} meet at a re-entrant corner with different eaves")]
    ReentrantCorner { a: usize, b: usize },
}

/// A face in plan: a counter-clockwise polygon in the building frame lying on the plane of
/// footprint edge `edge` (`None`: the flat roof at the eave height).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanFace {
    pub edge: Option<usize>,
    pub polygon: Vec<Vec2>,
}

/// Drops repeated and collinear vertices of a polygon, including 180° spikes (a skeleton
/// vertex sliding along an edge that has not started yet goes back over its own path).
#[must_use]
pub fn clean(mut poly: Vec<Vec2>) -> Vec<Vec2> {
    loop {
        let n = poly.len();
        if n < 3 {
            return poly;
        }
        let drop = (0..n).find(|&i| {
            let (a, b, c) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            // b is a repeat, or lies on the line a–c (straight on, or the tip of a spike).
            a.distance(b) <= 1e-9 || {
                let ac = c - a;
                let l = ac.length();
                l <= 1e-9 || (b - a).cross(ac).abs() / l <= 1e-7
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
/// [`RoofError::Degenerate`] when the skeleton cannot be computed.
pub fn plan_faces(input: &RoofInput) -> Result<Vec<PlanFace>, RoofError> {
    if input.sloped().next().is_none() {
        return Ok(vec![PlanFace {
            edge: None,
            polygon: input.outline.clone(),
        }]);
    }
    if is_convex(&input.outline) {
        let faces = envelope::envelope_faces(input);
        if faces.iter().all(|f| touches_own_eave(input, f)) {
            return Ok(faces);
        }
    } else {
        check_reentrant_corners(input)?;
    }
    skeleton_faces(input)
}

/// See [`RoofError::ReentrantCorner`].
///
/// # Errors
/// The first re-entrant corner whose edges do not start together.
pub fn check_reentrant_corners(input: &RoofInput) -> Result<(), RoofError> {
    let pts = &input.outline;
    let n = pts.len();
    for i in 0..n {
        let (a, b) = ((i + n - 1) % n, i);
        let (p, q, r) = (pts[a], pts[i], pts[(i + 1) % n]);
        if (q - p).cross(r - q) < 0.0 {
            let (pa, pb) = (&input.planes[a], &input.planes[b]);
            if !pa.is_sloped() || !pb.is_sloped() || (pa.base - pb.base).abs() > 1e-9 {
                return Err(RoofError::ReentrantCorner { a, b });
            }
        }
    }
    Ok(())
}

/// Whether a face has a stretch of its own outline edge (its eave) on its boundary.
#[must_use]
pub fn touches_own_eave(input: &RoofInput, face: &PlanFace) -> bool {
    let Some(e) = face.edge else {
        return true;
    };
    let n = input.outline.len();
    let eave = pv_geo::Segment::new(input.outline[e], input.outline[(e + 1) % n]);
    let m = face.polygon.len();
    (0..m).any(|k| {
        let (a, b) = (face.polygon[k], face.polygon[(k + 1) % m]);
        a.distance(b) > 1e-6
            && eave.distance_to_point(a) <= 1e-7
            && eave.distance_to_point(b) <= 1e-7
    })
}

/// Faces of any simple outline from the weighted straight skeleton: time = height, an edge
/// starts moving at its eave height at `1 / tan θ` (gable walls never move), so the arrival
/// time of its wavefront is its roof plane.
///
/// # Errors
/// [`RoofError::Degenerate`] when the skeleton cannot be computed.
pub fn skeleton_faces(input: &RoofInput) -> Result<Vec<PlanFace>, RoofError> {
    let edges: Vec<WaveEdge> = input
        .planes
        .iter()
        .map(|p| {
            if p.is_sloped() {
                WaveEdge {
                    speed: 1.0 / p.tan,
                    start: p.base,
                }
            } else {
                WaveEdge {
                    speed: 0.0,
                    start: p.base,
                }
            }
        })
        .collect();
    let sk = weighted_skeleton(&input.outline, &edges).map_err(|_| RoofError::Degenerate)?;
    Ok(sk
        .faces
        .into_iter()
        .enumerate()
        .flat_map(|(e, polys)| {
            polys.into_iter().filter_map(move |poly| {
                let poly = clean(poly);
                (poly.len() >= 3 && signed_area(&poly) > 1e-9).then_some(PlanFace {
                    edge: Some(e),
                    polygon: poly,
                })
            })
        })
        .collect())
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
            RoofError::Degenerate => "footprint",
            RoofError::ReentrantCorner { a, b } => {
                // Edges are numbered from 1 for people.
                return pv_model::CommandError::new(keys::ROOF_REENTRANT_CORNER)
                    .with_number("a", (a + 1) as f64)
                    .with_number("b", (b + 1) as f64);
            }
        };
        pv_model::CommandError::new(keys::ENTITY_INVALID).with_text("field", field)
    }
}
