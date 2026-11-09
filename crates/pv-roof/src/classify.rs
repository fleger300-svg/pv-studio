//! Edge kinds of the generated faces, and what the roof is (for the statics).
//!
//! - an edge on the roof outline is the face's own eave (`Eave`) or the sloped side of a
//!   face along a gable wall (`Verge`); on a flat roof, a parapet;
//! - an edge shared with another face is horizontal (`Ridge`), or sloped: a convex fold
//!   (`Hip`: the neighbour's plane rises above this face) or a concave one (`Valley`).

use pv_geo::{Segment, Vec2};
use pv_model::{RoofEdgeKind, RoofKind};

use crate::input::RoofInput;
use crate::PlanFace;

/// Distance (m) under which a point lies on an edge.
const ON_EDGE: f64 = 1e-7;

fn on_segment(s: &Segment, p: Vec2) -> bool {
    s.distance_to_point(p) <= ON_EDGE
}

/// The outline edge carrying the segment `a → b`, if any.
fn outline_edge(input: &RoofInput, a: Vec2, b: Vec2) -> Option<usize> {
    let n = input.outline.len();
    (0..n).find(|&i| {
        let s = Segment::new(input.outline[i], input.outline[(i + 1) % n]);
        on_segment(&s, a) && on_segment(&s, b)
    })
}

/// Height of face `f` above `p`.
pub fn face_z(input: &RoofInput, f: &PlanFace, p: Vec2) -> f64 {
    match f.edge {
        Some(e) => input.planes[e].z(p),
        None => input.eave_m,
    }
}

/// One kind per polygon edge of every face.
#[must_use]
pub fn edge_kinds(input: &RoofInput, faces: &[PlanFace]) -> Vec<Vec<RoofEdgeKind>> {
    faces
        .iter()
        .enumerate()
        .map(|(fi, face)| {
            let poly = &face.polygon;
            let n = poly.len();
            (0..n)
                .map(|k| {
                    let (a, b) = (poly[k], poly[(k + 1) % n]);
                    if face.edge.is_none() {
                        return RoofEdgeKind::Parapet;
                    }
                    if let Some(i) = outline_edge(input, a, b) {
                        return if face.edge == Some(i) {
                            RoofEdgeKind::Eave
                        } else {
                            RoofEdgeKind::Verge
                        };
                    }
                    let mid = (a + b) * 0.5;
                    let neighbour = faces.iter().enumerate().find(|(gi, g)| {
                        *gi != fi && {
                            let m = g.polygon.len();
                            (0..m).any(|j| {
                                on_segment(&Segment::new(g.polygon[j], g.polygon[(j + 1) % m]), mid)
                            })
                        }
                    });
                    let (za, zb) = (face_z(input, face, a), face_z(input, face, b));
                    let Some((_, g)) = neighbour else {
                        return RoofEdgeKind::Verge;
                    };
                    if (za - zb).abs() <= ON_EDGE {
                        return RoofEdgeKind::Ridge;
                    }
                    // A point just inside this face (left of a → b for a ccw polygon).
                    let d = b - a;
                    let q = mid + Vec2::new(-d.v, d.u) * (1e-3 / d.length());
                    if face_z(input, g, q) > face_z(input, face, q) {
                        RoofEdgeKind::Hip
                    } else {
                        RoofEdgeKind::Valley
                    }
                })
                .collect()
        })
        .collect()
}

/// What the generated roof is, for the statics' pressure coefficients.
#[must_use]
pub fn roof_kind(faces: &[PlanFace], kinds: &[Vec<RoofEdgeKind>]) -> RoofKind {
    if faces.iter().all(|f| f.edge.is_none()) {
        RoofKind::Flat
    } else if faces.len() == 1 {
        RoofKind::Monopitch
    } else if kinds.iter().flatten().any(|k| *k == RoofEdgeKind::Hip) {
        RoofKind::Hipped
    } else {
        RoofKind::Gable
    }
}
