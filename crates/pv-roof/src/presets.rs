//! Roof specifications for the usual roof types, on any footprint. The building's main axis
//! is the long side of the footprint's minimum-area rectangle: an edge within 45° of it is
//! a "long" edge (eaves of a gable roof), the others are "ends".

use pv_geo::{min_area_rect, Vec2};
use pv_model::{EdgeRoof, RoofSpec};

/// For each footprint edge: whether it runs along the building's main axis.
#[must_use]
pub fn long_edges(footprint: &[[f64; 2]]) -> Vec<bool> {
    let pts: Vec<Vec2> = footprint.iter().map(|p| Vec2::new(p[0], p[1])).collect();
    let axis = min_area_rect(&pts).map_or(Vec2::new(1.0, 0.0), |r| {
        Vec2::new(r.angle_rad.cos(), r.angle_rad.sin())
    });
    let n = pts.len();
    (0..n)
        .map(|i| {
            let d = pts[(i + 1) % n] - pts[i];
            // |cos| ≥ cos 45°; ties (exact 45°) go to the long side.
            d.dot(axis).abs() >= d.length() * std::f64::consts::FRAC_1_SQRT_2 - 1e-12
        })
        .collect()
}

fn spec(edges: Vec<EdgeRoof>, overhang_m: f64) -> RoofSpec {
    RoofSpec { edges, overhang_m }
}

/// Gable roof: long edges sloped, ends vertical.
#[must_use]
pub fn gable(footprint: &[[f64; 2]], pitch_deg: f64, overhang_m: f64) -> RoofSpec {
    spec(
        long_edges(footprint)
            .into_iter()
            .map(|l| {
                if l {
                    EdgeRoof::sloped(pitch_deg)
                } else {
                    EdgeRoof::gable()
                }
            })
            .collect(),
        overhang_m,
    )
}

/// Hipped roof: every edge sloped, the ends at `end_pitch_deg`.
#[must_use]
pub fn hipped(
    footprint: &[[f64; 2]],
    pitch_deg: f64,
    end_pitch_deg: f64,
    overhang_m: f64,
) -> RoofSpec {
    spec(
        long_edges(footprint)
            .into_iter()
            .map(|l| EdgeRoof::sloped(if l { pitch_deg } else { end_pitch_deg }))
            .collect(),
        overhang_m,
    )
}

/// Half-hipped roof: the ends rise at `end_pitch_deg` from a raised eave `end_eave_m`.
#[must_use]
pub fn half_hipped(
    footprint: &[[f64; 2]],
    pitch_deg: f64,
    end_pitch_deg: f64,
    end_eave_m: f64,
    overhang_m: f64,
) -> RoofSpec {
    spec(
        long_edges(footprint)
            .into_iter()
            .map(|l| {
                if l {
                    EdgeRoof::sloped(pitch_deg)
                } else {
                    EdgeRoof {
                        pitch_deg: end_pitch_deg,
                        eave_height_m: Some(end_eave_m),
                    }
                }
            })
            .collect(),
        overhang_m,
    )
}

/// Monopitch roof draining over footprint edge `low_edge`.
#[must_use]
pub fn monopitch(
    footprint: &[[f64; 2]],
    low_edge: usize,
    pitch_deg: f64,
    overhang_m: f64,
) -> RoofSpec {
    spec(
        (0..footprint.len())
            .map(|i| {
                if i == low_edge {
                    EdgeRoof::sloped(pitch_deg)
                } else {
                    EdgeRoof::gable()
                }
            })
            .collect(),
        overhang_m,
    )
}

/// Flat roof.
#[must_use]
pub fn flat(footprint: &[[f64; 2]], overhang_m: f64) -> RoofSpec {
    spec(vec![EdgeRoof::gable(); footprint.len()], overhang_m)
}
