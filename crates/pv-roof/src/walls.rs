//! Walls of a building, from the ground up to its roof: one vertical polygon per footprint
//! edge, whose top follows the roof faces above the edge (eaves, gable triangles, the raised
//! eave of a half-hip). Works under generated faces and under faces placed by hand alike.

use pv_geo::{Polygon, Vec2};
use pv_model::{Building, RoofFace, WallSummary};

/// A face in plan with its plane.
struct PlanOf {
    polygon: Polygon,
    vertices: Vec<Vec2>,
    origin: [f64; 3],
    normal: [f64; 3],
}

impl PlanOf {
    fn new(face: &RoofFace) -> Option<Self> {
        let frame = face.frame();
        let uv: Vec<[f64; 2]> = match &face.outline {
            Some(o) => o.vertices_uv.clone(),
            None => vec![
                [0.0, 0.0],
                [face.width_m, 0.0],
                [face.width_m, face.height_m],
                [0.0, face.height_m],
            ],
        };
        let vertices: Vec<Vec2> = uv
            .iter()
            .map(|&[u, v]| {
                let w = frame.to_world(u, v, 0.0);
                Vec2::new(w[0], w[1])
            })
            .collect();
        if frame.normal[2] <= 1e-9 {
            return None; // vertical: no height above a plan point
        }
        Some(Self {
            polygon: Polygon::new(vertices.clone()).ok()?,
            vertices,
            origin: frame.origin,
            normal: frame.normal,
        })
    }

    fn z(&self, p: Vec2) -> f64 {
        let (o, n) = (self.origin, self.normal);
        o[2] - (n[0] * (p.u - o[0]) + n[1] * (p.v - o[1])) / n[2]
    }

    fn covers(&self, p: Vec2) -> bool {
        self.polygon.contains_point(p) || self.polygon.distance_to_boundary(p) <= 1e-6
    }
}

/// Parameters `t ∈ [0, 1]` along `a → b` where the segment meets the polygon's edges.
fn crossings(a: Vec2, b: Vec2, poly: &[Vec2], out: &mut Vec<f64>) {
    let d = b - a;
    let len2 = d.dot(d);
    let n = poly.len();
    for i in 0..n {
        let (c, e) = (poly[i], poly[(i + 1) % n]);
        // Vertices on the wall line.
        for p in [c, e] {
            let t = (p - a).dot(d) / len2;
            let foot = a + d * t;
            if foot.distance(p) <= 1e-6 {
                out.push(t);
            }
        }
        // Proper crossing.
        let f = e - c;
        let denom = d.cross(f);
        if denom.abs() > 1e-12 {
            let t = (c - a).cross(f) / denom;
            let s = (c - a).cross(d) / denom;
            if (0.0..=1.0).contains(&s) {
                out.push(t);
            }
        }
    }
}

/// The walls of `building` under `faces` (its roof faces).
#[must_use]
pub fn walls(building: &Building, faces: &[&RoofFace]) -> Vec<WallSummary> {
    let o = building.origin;
    let plans: Vec<PlanOf> = faces.iter().filter_map(|f| PlanOf::new(f)).collect();
    let fp: Vec<Vec2> = building
        .footprint
        .iter()
        .map(|p| Vec2::new(o[0] + p[0], o[1] + p[1]))
        .collect();
    let eave = o[2] + building.eave_height_m;
    let n = fp.len();
    (0..n)
        .map(|i| {
            let (a, b) = (fp[i], fp[(i + 1) % n]);
            let len = a.distance(b);
            let mut ts = vec![0.0, 1.0];
            for p in &plans {
                crossings(a, b, &p.vertices, &mut ts);
            }
            let mut ts: Vec<f64> = ts.into_iter().filter(|t| (0.0..=1.0).contains(t)).collect();
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() * len <= 1e-6);
            let at = |t: f64| a + (b - a) * t;
            let mut profile: Vec<[f64; 2]> = Vec::new();
            for w in ts.windows(2) {
                let mid = at((w[0] + w[1]) / 2.0);
                // The highest face above the interval (faces touching the wall line count).
                let top = plans
                    .iter()
                    .filter(|p| p.covers(mid))
                    .max_by(|p, q| p.z(mid).total_cmp(&q.z(mid)));
                for t in [w[0], w[1]] {
                    let z = top.map_or(eave, |p| p.z(at(t)));
                    let pt = [t * len, z];
                    if profile
                        .last()
                        .is_none_or(|q| (q[0] - pt[0]).abs() > 1e-9 || (q[1] - pt[1]).abs() > 1e-9)
                    {
                        profile.push(pt);
                    }
                }
            }
            WallSummary {
                building_id: building.id,
                start: [a.u, a.v],
                end: [b.u, b.v],
                base_z: o[2],
                profile,
            }
        })
        .collect()
}
