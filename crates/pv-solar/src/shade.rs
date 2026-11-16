//! Near shading by the project's own geometry: roof faces, walls of every building and roof
//! obstacles (prisms) cast shadows on the modules.
//!
//! - Beam: for each sun direction bin (5° × 5°), the shaded fraction of every array is the
//!   share of blocked rays from 5 points of each module (centre and quarter points), averaged
//!   over the array's modules. Bins are computed lazily, only for the directions the sun
//!   actually takes.
//! - Diffuse: the sky view factor of each array (cosine-weighted hemisphere sampling around
//!   the module normal), averaged over its modules.
//!
//! Linear in the shaded area: bypass-diode effects (a shaded cell costing a whole substring)
//! are not modelled, so partial shading is underestimated. Modules do not shade each other
//! (tilted rows on flat roofs: inter-row shading not modelled).

use std::collections::HashMap;

use pv_model::{ArrayId, BuildingId, ModuleState, Project, RoofFaceId};

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A triangle that casts shadows, with the face it belongs to (a face never shades itself).
struct Tri {
    a: V3,
    e1: V3,
    e2: V3,
    face: Option<RoofFaceId>,
    /// Walls: their building (a building's walls never shade its own roof: they stand
    /// under it).
    wall_of: Option<BuildingId>,
}

/// Möller–Trumbore: does the ray `o + t·d` (t > ε) hit the triangle?
fn hits(t: &Tri, o: V3, d: V3) -> bool {
    let p = cross(d, t.e2);
    let det = dot(t.e1, p);
    if det.abs() < 1e-12 {
        return false;
    }
    let inv = 1.0 / det;
    let s = sub(o, t.a);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, t.e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    dot(t.e2, q) * inv > 1e-3
}

fn fan(points: &[V3], face: Option<RoofFaceId>, wall_of: Option<BuildingId>, out: &mut Vec<Tri>) {
    for i in 1..points.len().saturating_sub(1) {
        out.push(Tri {
            a: points[0],
            e1: sub(points[i], points[0]),
            e2: sub(points[i + 1], points[0]),
            face,
            wall_of,
        });
    }
}

/// Unit vector towards the sun (x east, y north, z up).
#[must_use]
pub fn sun_vector(azimuth_deg: f64, elevation_deg: f64) -> V3 {
    let (az, el) = (azimuth_deg.to_radians(), elevation_deg.to_radians());
    [az.sin() * el.cos(), az.cos() * el.cos(), el.sin()]
}

/// Index of the 5° × 5° bin of a sun direction.
#[must_use]
pub fn bin_of(azimuth_deg: f64, elevation_deg: f64) -> (u16, u16) {
    (
        (azimuth_deg.rem_euclid(360.0) / 5.0) as u16 % 72,
        (elevation_deg.clamp(0.0, 89.999) / 5.0) as u16,
    )
}

struct Receiver {
    face: RoofFaceId,
    building: BuildingId,
    /// Sample points per module.
    modules: Vec<[V3; 5]>,
    normal: V3,
}

/// Shading of every array of a project.
pub struct ShadeModel {
    tris: Vec<Tri>,
    receivers: HashMap<ArrayId, Receiver>,
    beam: HashMap<(ArrayId, (u16, u16)), f64>,
    diffuse: HashMap<ArrayId, f64>,
}

impl ShadeModel {
    /// Builds the occluders and receivers of `project`.
    #[must_use]
    pub fn new(project: &Project) -> Self {
        let mut tris = Vec::new();
        for f in &project.roof_faces {
            let fr = f.frame();
            let world = |u: f64, v: f64| add(fr.origin, add(mul(fr.axis_u, u), mul(fr.axis_v, v)));
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
            let pts: Vec<V3> = outline.iter().map(|p| world(p[0], p[1])).collect();
            fan(&pts, Some(f.id), None, &mut tris);
            for o in f.outline.iter().flat_map(|o| &o.obstacles) {
                let base: Vec<V3> = o.footprint_uv.iter().map(|p| world(p[0], p[1])).collect();
                let top: Vec<V3> = base
                    .iter()
                    .map(|&p| add(p, mul(fr.normal, o.height_m)))
                    .collect();
                fan(&top, None, None, &mut tris);
                for i in 0..base.len() {
                    let j = (i + 1) % base.len();
                    fan(&[base[i], base[j], top[j], top[i]], None, None, &mut tris);
                }
            }
        }
        for b in &project.buildings {
            let faces: Vec<&pv_model::RoofFace> = project
                .roof_faces
                .iter()
                .filter(|f| f.building_id == b.id)
                .collect();
            for w in pv_roof_walls(b, &faces) {
                let (s, e) = (w.start, w.end);
                let len = ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2))
                    .sqrt()
                    .max(1e-9);
                let at = |sd: f64, z: f64| -> V3 {
                    [
                        s[0] + (e[0] - s[0]) * sd / len,
                        s[1] + (e[1] - s[1]) * sd / len,
                        z,
                    ]
                };
                let mut poly: Vec<V3> = vec![at(0.0, w.base_z), at(len, w.base_z)];
                poly.extend(w.profile.iter().rev().map(|p| at(p[0], p[1])));
                fan(&poly, None, Some(b.id), &mut tris);
            }
        }
        let mut receivers = HashMap::new();
        for a in &project.arrays {
            let Some(f) = project.roof_faces.iter().find(|f| f.id == a.roof_face_id) else {
                continue;
            };
            let fr = f.frame();
            let (hu, hv) = a.half_footprint();
            let lift = mul(fr.normal, 0.05);
            let world = |u: f64, v: f64| {
                add(
                    add(fr.origin, add(mul(fr.axis_u, u), mul(fr.axis_v, v))),
                    lift,
                )
            };
            let modules = project
                .modules
                .iter()
                .filter(|m| m.array_id == a.id && m.state == ModuleState::Active)
                .map(|m| {
                    [
                        world(m.u, m.v),
                        world(m.u - hu / 2.0, m.v - hv / 2.0),
                        world(m.u + hu / 2.0, m.v - hv / 2.0),
                        world(m.u + hu / 2.0, m.v + hv / 2.0),
                        world(m.u - hu / 2.0, m.v + hv / 2.0),
                    ]
                })
                .collect();
            receivers.insert(
                a.id,
                Receiver {
                    face: f.id,
                    building: f.building_id,
                    modules,
                    normal: fr.normal,
                },
            );
        }
        Self {
            tris,
            receivers,
            beam: HashMap::new(),
            diffuse: HashMap::new(),
        }
    }

    fn blocked(&self, r: &Receiver, o: V3, d: V3) -> bool {
        self.tris
            .iter()
            .any(|t| t.face != Some(r.face) && t.wall_of != Some(r.building) && hits(t, o, d))
    }

    /// Shaded fraction of the beam on `array` for a sun in `bin` (bin centre direction).
    pub fn beam_shade(&mut self, array: ArrayId, bin: (u16, u16)) -> f64 {
        if let Some(&s) = self.beam.get(&(array, bin)) {
            return s;
        }
        let s = self.receivers.get(&array).map_or(0.0, |r| {
            let d = sun_vector(f64::from(bin.0) * 5.0 + 2.5, f64::from(bin.1) * 5.0 + 2.5);
            let (mut hit, mut n) = (0usize, 0usize);
            for m in &r.modules {
                for &p in m {
                    n += 1;
                    hit += usize::from(self.blocked(r, p, d));
                }
            }
            if n == 0 {
                0.0
            } else {
                hit as f64 / n as f64
            }
        });
        self.beam.insert((array, bin), s);
        s
    }

    /// Sky view factor of `array` (1 = unobstructed), cosine-weighted around its normal.
    pub fn diffuse_factor(&mut self, array: ArrayId) -> f64 {
        if let Some(&f) = self.diffuse.get(&array) {
            return f;
        }
        let f = self.receivers.get(&array).map_or(1.0, |r| {
            let (mut seen, mut total) = (0.0, 0.0);
            // Centre points only (diffuse varies slowly); directions above the horizon.
            for m in &r.modules {
                let p = m[0];
                for el in (1..9).map(|k| f64::from(k) * 10.0 - 5.0) {
                    for az in (0..24).map(|k| f64::from(k) * 15.0) {
                        let d = sun_vector(az, el);
                        let w = dot(d, r.normal).max(0.0) * el.to_radians().cos();
                        if w <= 0.0 {
                            continue;
                        }
                        total += w;
                        if !self.blocked(r, p, d) {
                            seen += w;
                        }
                    }
                }
            }
            if total > 0.0 {
                seen / total
            } else {
                1.0
            }
        });
        self.diffuse.insert(array, f);
        f
    }
}

/// Walls of a building (the roof generator's own rule).
fn pv_roof_walls(
    b: &pv_model::Building,
    faces: &[&pv_model::RoofFace],
) -> Vec<pv_model::WallSummary> {
    pv_roof::walls(b, faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_triangle() {
        let t = Tri {
            a: [0.0, 0.0, 1.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
            face: None,
            wall_of: None,
        };
        assert!(hits(&t, [0.2, 0.2, 0.0], [0.0, 0.0, 1.0]));
        assert!(!hits(&t, [0.8, 0.8, 0.0], [0.0, 0.0, 1.0]));
        assert!(
            !hits(&t, [0.2, 0.2, 2.0], [0.0, 0.0, 1.0]),
            "behind the origin"
        );
        assert_eq!(bin_of(182.0, 31.0), (36, 6));
        let s = sun_vector(180.0, 90.0);
        assert!((s[2] - 1.0).abs() < 1e-12);
    }
}
