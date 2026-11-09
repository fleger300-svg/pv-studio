//! Convex footprints: the roof is the lower envelope of the edge planes,
//! `z(p) = min_e z_e(p)`, and the face of edge `e` is the outline cut by the half-planes
//! `z_e ≤ z_f` (`f ≠ e`) — each one linear in `p`:
//!
//! `(tan θ_e n_e − tan θ_f n_f) · p ≤ (h_f − tan θ_f n_f·a_f) − (h_e − tan θ_e n_e·a_e)`.
//!
//! O(n²) convex clips, exact up to rounding. Vertical edges (90°) never win the minimum: they
//! carry no face and become gable walls.

use pv_geo::{clip_convex, HalfPlane};

use crate::input::{EdgePlane, RoofInput};
use crate::PlanFace;

/// Points this close to a cutting line (m) count as on it: neighbouring faces keep their
/// shared edge exactly.
const CLIP_TOL: f64 = 1e-9;

fn half_plane(e: &EdgePlane, f: &EdgePlane) -> HalfPlane {
    let normal = e.inward * e.tan - f.inward * f.tan;
    let offset =
        (f.base - f.tan * f.inward.dot(f.anchor)) - (e.base - e.tan * e.inward.dot(e.anchor));
    HalfPlane { normal, offset }
}

/// Faces of a convex outline, in edge order (edges without a face are skipped).
#[must_use]
pub fn envelope_faces(input: &RoofInput) -> Vec<PlanFace> {
    let sloped: Vec<usize> = input.sloped().collect();
    let mut faces = Vec::new();
    for &e in &sloped {
        let pe = &input.planes[e];
        let mut poly = input.outline.clone();
        for &f in sloped.iter().filter(|&&f| f != e) {
            let h = half_plane(pe, &input.planes[f]);
            let len = h.normal.length();
            if len < 1e-12 {
                // Parallel planes: one is entirely below the other, or they coincide (the
                // lower edge index owns the shared face).
                if h.offset < -CLIP_TOL || (h.offset.abs() <= CLIP_TOL && f < e) {
                    poly.clear();
                }
            } else {
                let unit = HalfPlane {
                    normal: h.normal * (1.0 / len),
                    offset: h.offset / len,
                };
                poly = clip_convex(&poly, unit, CLIP_TOL);
            }
            if poly.is_empty() {
                break;
            }
        }
        let poly = crate::clean(poly);
        if poly.len() >= 3 {
            faces.push(PlanFace {
                edge: Some(e),
                polygon: poly,
            });
        }
    }
    faces
}
