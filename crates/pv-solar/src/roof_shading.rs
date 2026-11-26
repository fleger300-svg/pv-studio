//! Shading map of the roof faces: on a grid over each face (free area included), the share
//! of the yearly irradiation of a surface lying on the roof that the far horizon and the
//! near shading take — where modules would lose least. Same chain as [`crate::simulate`]
//! (horizon first, then the near shading of the beam, circumsolar and sky diffuse), for the
//! face's own orientation.

use std::collections::HashMap;

use pv_model::{Project, RoofFaceId, HOURS_PER_YEAR};
use serde::Serialize;
use specta::Type;

use crate::sun::{extraterrestrial, hay_davies, position};
use crate::{diffuse_horizontal, month_of, shade, NotSimulated};

/// Points sampled over all the faces at most (the cell grows on large roofs).
const MAX_POINTS: f64 = 8000.0;
/// Smallest cell [m].
const MIN_CELL_M: f64 = 0.25;
/// Height of the sampled points over the roof [m].
const LIFT_M: f64 = 0.05;
/// A grid value for a cell outside the face (or under a roof obstacle).
pub const OUTSIDE: i16 = -1;

/// Shading over one roof face, on a grid in its own `(u, v)` frame.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FaceShadingGrid {
    pub face_id: RoofFaceId,
    /// `(u, v)` of the grid's corner (cell `(0, 0)` spans `u0..u0 + cell`, `v0..v0 + cell`).
    pub origin_uv: [f64; 2],
    pub cell_m: f64,
    pub cols: u32,
    pub rows: u32,
    /// Loss share of each cell in hundredths of a percent (row by row from `v0`, each row
    /// from `u0`); [`OUTSIDE`] off the face.
    pub losses: Vec<i16>,
}

/// Shading over every roof face of a project.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofShadingMap {
    pub faces: Vec<FaceShadingGrid>,
}

/// Is `p` inside the polygon `poly` (even-odd rule)?
fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
    }
    c
}

/// Irradiation of one face orientation over the year [W/m²·h].
struct FaceSums {
    poa: f64,
    horizon: f64,
    /// Sky diffuse after the horizon (shaded by the sky view of each point).
    iso: f64,
    /// Beam and circumsolar after the horizon, by sun bin and month.
    direct: HashMap<(u16, u16), [f64; 12]>,
}

/// Shading map of every roof face of `project` over its weather year.
///
/// # Errors
/// [`NotSimulated::NoWeather`] without weather.
pub fn roof_shading(project: &Project, albedo: f64) -> Result<RoofShadingMap, NotSimulated> {
    let w = project
        .site
        .weather
        .as_ref()
        .ok_or(NotSimulated::NoWeather)?;
    let (lat, lon) = project
        .site
        .location
        .as_ref()
        .map_or((w.lat_deg, w.lon_deg), |l| (l.lat_deg, l.lon_deg));
    let model = shade::ShadeModel::new(project);
    let faces = &project.roof_faces;

    // Yearly sums per face orientation.
    let mut sums: Vec<FaceSums> = faces
        .iter()
        .map(|_| FaceSums {
            poa: 0.0,
            horizon: 0.0,
            iso: 0.0,
            direct: HashMap::new(),
        })
        .collect();
    let views: Vec<f64> = faces
        .iter()
        .map(|f| model.horizon_view_around(f.frame().normal))
        .collect();
    for h in 0..HOURS_PER_YEAR {
        let ghi = f64::from(w.ghi_w_m2[h]);
        if ghi <= 0.0 {
            continue;
        }
        let day = (h / 24) as u32 + 1;
        let sun = position(lat, lon, day, (h % 24) as f64 + 0.5);
        let e0 = extraterrestrial(day);
        let dhi = diffuse_horizontal(w, h, ghi, &sun, e0);
        let month = month_of(h);
        let elevation = 90.0 - sun.zenith_deg;
        let behind = elevation > 0.0 && model.below_horizon(sun.azimuth_deg, elevation);
        for ((f, s), view) in faces.iter().zip(&mut sums).zip(&views) {
            let poa = hay_davies(&sun, ghi, dhi, e0, f.tilt_deg, f.azimuth_deg, albedo);
            let gpoa = poa.total();
            if gpoa <= 0.0 {
                continue;
            }
            let direct = if behind {
                0.0
            } else {
                poa.beam + poa.circumsolar
            };
            let h_iso = (poa.sky_diffuse - poa.circumsolar) * view;
            s.poa += gpoa;
            s.horizon += gpoa - (direct + h_iso + poa.ground);
            s.iso += h_iso;
            if direct > 0.0 && elevation > 0.0 {
                s.direct
                    .entry(shade::bin_of(sun.azimuth_deg, elevation))
                    .or_insert([0.0; 12])[month] += direct;
            }
        }
    }

    // One cell size for the whole project, so that the faces read alike.
    let area: f64 = faces.iter().map(|f| f.width_m * f.height_m).sum();
    let cell = ((area / MAX_POINTS).sqrt() / MIN_CELL_M).ceil().max(1.0) * MIN_CELL_M;
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);

    let mut out = Vec::with_capacity(faces.len());
    for (f, s) in faces.iter().zip(&sums) {
        if s.poa <= 0.0 {
            continue;
        }
        let fr = f.frame();
        let outline: Vec<[f64; 2]> = f.outline.as_ref().map_or_else(
            || {
                vec![
                    [0.0, 0.0],
                    [f.width_m, 0.0],
                    [f.width_m, f.height_m],
                    [0.0, f.height_m],
                ]
            },
            |o| o.vertices_uv.clone(),
        );
        let obstacles: Vec<&[[f64; 2]]> = f
            .outline
            .iter()
            .flat_map(|o| &o.obstacles)
            .map(|o| o.footprint_uv.as_slice())
            .collect();
        let (mut u0, mut v0, mut u1, mut v1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &outline {
            u0 = u0.min(p[0]);
            v0 = v0.min(p[1]);
            u1 = u1.max(p[0]);
            v1 = v1.max(p[1]);
        }
        if outline.len() < 3 || u1 <= u0 || v1 <= v0 {
            continue;
        }
        let cols = ((u1 - u0) / cell).ceil() as usize;
        let rows = ((v1 - v0) / cell).ceil() as usize;
        let directions: Vec<([f64; 3], [f64; 12])> = s
            .direct
            .iter()
            .map(|(&(a, e), m)| {
                (
                    shade::sun_vector(f64::from(a) * 5.0 + 2.5, f64::from(e) * 5.0 + 2.5),
                    *m,
                )
            })
            .collect();
        let loss_at = |k: usize| -> i16 {
            let (r, c) = (k / cols, k % cols);
            let uv = [u0 + (c as f64 + 0.5) * cell, v0 + (r as f64 + 0.5) * cell];
            if !inside(&outline, uv) || obstacles.iter().any(|o| inside(o, uv)) {
                return OUTSIDE;
            }
            let p = fr.to_world(uv[0], uv[1], LIFT_M);
            let beam: f64 = directions
                .iter()
                .map(|(d, m)| model.point_beam_loss(f.id, f.building_id, p, *d, m))
                .sum();
            let sky = model.point_sky_view(f.id, f.building_id, p, fr.normal);
            let loss = (s.horizon + beam + s.iso * (1.0 - sky)) / s.poa;
            (loss.clamp(0.0, 1.0) * 10_000.0).round() as i16
        };
        let n = cols * rows;
        let mut losses = vec![OUTSIDE; n];
        let chunk = n.div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            for (i, part) in losses.chunks_mut(chunk).enumerate() {
                let loss_at = &loss_at;
                scope.spawn(move || {
                    for (j, x) in part.iter_mut().enumerate() {
                        *x = loss_at(i * chunk + j);
                    }
                });
            }
        });
        out.push(FaceShadingGrid {
            face_id: f.id,
            origin_uv: [u0, v0],
            cell_m: cell,
            cols: cols as u32,
            rows: rows as u32,
            losses,
        });
    }
    Ok(RoofShadingMap { faces: out })
}

#[cfg(test)]
mod tests {
    #[test]
    fn point_in_polygon() {
        let l = [[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]];
        assert!(super::inside(&l, [0.5, 1.5]));
        assert!(super::inside(&l, [1.5, 0.5]));
        assert!(!super::inside(&l, [1.5, 1.5]));
        assert!(!super::inside(&l, [3.0, 0.5]));
    }
}
