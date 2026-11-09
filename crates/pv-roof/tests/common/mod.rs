//! Shared helpers: buildings, a deterministic RNG and the invariant checker of a generated
//! roof (valid for the envelope and the skeleton alike).

#![allow(dead_code)]

use pv_geo::{convex_hull, intersection_area, signed_area, Vec2};
use pv_model::{Building, BuildingId, RoofEdgeKind, RoofKind, RoofSpec};
use pv_roof::{classify, GeneratedRoof};
use uuid::Uuid;

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

pub fn building(footprint: Vec<[f64; 2]>, spec: RoofSpec) -> Building {
    Building {
        id: BuildingId(Uuid::from_u128(1)),
        name: "B".into(),
        origin: [100.0, -50.0, 2.0],
        footprint,
        size_m: [1.0, 1.0],
        eave_height_m: 6.0,
        ridge_height_m: 6.0,
        roof_kind: RoofKind::Gable,
        roof: Some(spec),
        site: None,
    }
}

pub fn rect(l: f64, w: f64) -> Vec<[f64; 2]> {
    pv_model::rectangle_footprint([l, w])
}

/// A random convex footprint (3 to 10 vertices, edges ≥ 0.5 m).
pub fn convex_footprint(rng: &mut Rng) -> Vec<[f64; 2]> {
    loop {
        let n = 3 + rng.below(8) as usize;
        let (rx, ry) = (rng.range(4.0, 20.0), rng.range(4.0, 20.0));
        let rot = rng.range(0.0, std::f64::consts::TAU);
        let pts: Vec<Vec2> = (0..n)
            .map(|_| {
                let a = rng.range(0.0, std::f64::consts::TAU);
                let r = rng.range(0.8, 1.0);
                Vec2::new(rx * r * a.cos(), ry * r * a.sin()).rotated(rot)
            })
            .collect();
        let hull = convex_hull(&pts);
        let m = hull.len();
        if m >= 3 && (0..m).all(|i| hull[i].distance(hull[(i + 1) % m]) >= 0.5) {
            return hull.iter().map(|p| [p.u, p.v]).collect();
        }
    }
}

fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= tol)
}

/// Checks the invariants of a generated roof; returns a description of the first failure.
pub fn check_roof(b: &Building, roof: &GeneratedRoof) -> Result<(), String> {
    let input = &roof.input;
    let outline_area = signed_area(&input.outline);
    let tol_area = 1e-7 * outline_area.max(1.0);
    // 1. The faces tile the roof outline: areas add up, no overlap.
    let plan_areas: Vec<f64> = roof
        .plan_faces
        .iter()
        .map(|f| signed_area(&f.polygon))
        .collect();
    if let Some(i) = plan_areas.iter().position(|a| *a <= 0.0) {
        return Err(format!("face {i} is not counter-clockwise"));
    }
    let sum: f64 = plan_areas.iter().sum();
    if (sum - outline_area).abs() > tol_area {
        return Err(format!("faces cover {sum} m², outline {outline_area} m²"));
    }
    for i in 0..roof.plan_faces.len() {
        for j in i + 1..roof.plan_faces.len() {
            let x = intersection_area(&roof.plan_faces[i].polygon, &roof.plan_faces[j].polygon);
            if x > tol_area {
                return Err(format!("faces {i} and {j} overlap by {x} m²"));
            }
        }
    }
    // 2. Each RoofFace reproduces its plan face: frame round trip and true area.
    let o = b.origin;
    for (k, (pf, f)) in roof.plan_faces.iter().zip(&roof.faces).enumerate() {
        let frame = f.frame();
        let uv: Vec<[f64; 2]> = match &f.outline {
            Some(s) => s.vertices_uv.clone(),
            None => vec![
                [0.0, 0.0],
                [f.width_m, 0.0],
                [f.width_m, f.height_m],
                [0.0, f.height_m],
            ],
        };
        for &p in &pf.polygon {
            let w = [
                o[0] + p.u,
                o[1] + p.v,
                o[2] + classify::face_z(input, pf, p),
            ];
            let (u, v, n) = frame.to_local(w);
            if n.abs() > 1e-7 {
                return Err(format!("face {k}: vertex {w:?} off its plane by {n}"));
            }
            if !uv
                .iter()
                .any(|q| (q[0] - u).abs() < 1e-7 && (q[1] - v).abs() < 1e-7)
            {
                return Err(format!("face {k}: vertex ({u}, {v}) not in the outline"));
            }
        }
        let uv_pts: Vec<Vec2> = uv.iter().map(|q| Vec2::new(q[0], q[1])).collect();
        let true_area = signed_area(&uv_pts);
        let projected = true_area * f.tilt_deg.to_radians().cos();
        if (projected - plan_areas[k]).abs() > 1e-7 * plan_areas[k].max(1.0) {
            return Err(format!(
                "face {k}: area·cos θ = {projected}, plan {}",
                plan_areas[k]
            ));
        }
    }
    // 3. Interior edges are shared in 3D; eaves sit on their plane's base.
    let kinds = classify::edge_kinds(input, &roof.plan_faces);
    let edges3: Vec<Vec<([f64; 3], [f64; 3])>> = roof
        .plan_faces
        .iter()
        .map(|pf| {
            let n = pf.polygon.len();
            (0..n)
                .map(|i| {
                    let (a, c) = (pf.polygon[i], pf.polygon[(i + 1) % n]);
                    (
                        [a.u, a.v, classify::face_z(input, pf, a)],
                        [c.u, c.v, classify::face_z(input, pf, c)],
                    )
                })
                .collect()
        })
        .collect();
    for (fi, ks) in kinds.iter().enumerate() {
        for (ei, k) in ks.iter().enumerate() {
            let (a, c) = edges3[fi][ei];
            match k {
                RoofEdgeKind::Hip | RoofEdgeKind::Ridge | RoofEdgeKind::Valley => {
                    // The neighbour may split the edge: each endpoint lies on a neighbour's
                    // 3D edge.
                    let on_other = |p: [f64; 3]| {
                        edges3.iter().enumerate().any(|(gi, es)| {
                            gi != fi
                                && es.iter().any(|&(x, y)| {
                                    let d = [y[0] - x[0], y[1] - x[1], y[2] - x[2]];
                                    let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                                    let t = ((p[0] - x[0]) * d[0]
                                        + (p[1] - x[1]) * d[1]
                                        + (p[2] - x[2]) * d[2])
                                        / l2;
                                    (-1e-9..=1.0 + 1e-9).contains(&t)
                                        && close(
                                            p,
                                            [x[0] + d[0] * t, x[1] + d[1] * t, x[2] + d[2] * t],
                                            1e-7,
                                        )
                                })
                        })
                    };
                    if !on_other(a) || !on_other(c) {
                        return Err(format!("face {fi} edge {ei} ({k:?}) is not shared in 3D"));
                    }
                }
                RoofEdgeKind::Eave => {
                    let e = roof.plan_faces[fi].edge.expect("sloped");
                    let base = input.planes[e].base;
                    if (a[2] - base).abs() > 1e-7 || (c[2] - base).abs() > 1e-7 {
                        return Err(format!("face {fi}: eave not at {base}"));
                    }
                }
                _ => {}
            }
        }
    }
    // 4. Derived fields.
    let top = edges3
        .iter()
        .flatten()
        .flat_map(|(a, c)| [a[2], c[2]])
        .fold(f64::NEG_INFINITY, f64::max)
        .max(b.eave_height_m);
    if (roof.ridge_height_m - top).abs() > 1e-9 {
        return Err("ridge height".into());
    }
    Ok(())
}

/// Number of random cases: `default`, or `ROOF_CASES` for longer campaigns.
pub fn cases(default: usize) -> usize {
    std::env::var("ROOF_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
