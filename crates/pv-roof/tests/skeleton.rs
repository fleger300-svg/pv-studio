//! E2.4: concave footprints (weighted straight skeleton), with the convex envelope as oracle.

mod common;

use std::collections::BTreeMap;

use common::*;
use pv_geo::{signed_area, Vec2};
use pv_model::{EdgeRoof, RoofEdgeKind, RoofKind, RoofSpec};
use pv_roof::{envelope, generate, presets, skeleton_faces, touches_own_eave, RoofInput};

fn area_by_edge(faces: &[pv_roof::PlanFace]) -> BTreeMap<usize, f64> {
    let mut m = BTreeMap::new();
    for f in faces {
        *m.entry(f.edge.unwrap_or(usize::MAX)).or_insert(0.0) += signed_area(&f.polygon);
    }
    m
}

fn random_edges(rng: &mut Rng, n: usize, vertical: u64, raised: u64) -> Vec<EdgeRoof> {
    (0..n)
        .map(|_| {
            if rng.below(vertical) == 0 {
                EdgeRoof::gable()
            } else {
                EdgeRoof {
                    pitch_deg: rng.range(10.0, 55.0),
                    eave_height_m: (rng.below(raised) == 0).then(|| rng.range(6.0, 7.5)),
                }
            }
        })
        .collect()
}

/// The skeleton and the envelope agree on 1 000 random convex roofs (same area per edge)
/// whenever every envelope face touches its eave; otherwise the skeleton roof is used, and
/// all its faces do.
#[test]
fn skeleton_equals_envelope_on_convex_roofs() {
    let mut rng = Rng(0x5EED_0001);
    let mut detached = 0;
    for case in 0..cases(1000) {
        let fp = convex_footprint(&mut rng);
        let n = fp.len();
        let edges = random_edges(&mut rng, n, 5, 8);
        if edges.iter().all(|e| !e.is_sloped()) {
            continue;
        }
        let overhang_m = if rng.below(2) == 0 {
            0.0
        } else {
            rng.range(0.1, 0.6)
        };
        let b = building(fp, RoofSpec { edges, overhang_m });
        let input = RoofInput::new(&b).unwrap();
        let env_faces = envelope::envelope_faces(&input);
        let sk_faces = skeleton_faces(&input).unwrap_or_else(|e| panic!("case {case}: {e}\n{b:?}"));
        for f in &sk_faces {
            assert!(
                touches_own_eave(&input, f),
                "case {case}: skeleton face off its eave"
            );
        }
        if !env_faces.iter().all(|f| touches_own_eave(&input, f)) {
            detached += 1;
            let roof = generate(&b).unwrap();
            check_roof(&b, &roof).unwrap_or_else(|e| panic!("case {case}: {e}\n{b:?}"));
            continue;
        }
        let env = area_by_edge(&env_faces);
        let sk = area_by_edge(&sk_faces);
        let total: f64 = env.values().sum();
        for e in env.keys().chain(sk.keys()) {
            let (a, s) = (
                env.get(e).copied().unwrap_or(0.0),
                sk.get(e).copied().unwrap_or(0.0),
            );
            assert!(
                (a - s).abs() <= 1e-6 * total,
                "case {case}, edge {e}: envelope {a} m², skeleton {s} m²\n{b:?}"
            );
        }
    }
    eprintln!(
        "{detached} / {} envelopes with a face detached from its eave",
        cases(1000)
    );
    assert!(detached * 10 < cases(1000), "{detached}");
}

#[test]
fn l_shaped_house_has_hips_and_a_valley() {
    // Arms 10 m wide; equal pitch 30°.
    let fp = vec![
        [0.0, 0.0],
        [20.0, 0.0],
        [20.0, 10.0],
        [10.0, 10.0],
        [10.0, 20.0],
        [0.0, 20.0],
    ];
    let b = building(fp.clone(), presets::hipped(&fp, 30.0, 30.0, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    assert_eq!(roof.roof_kind, RoofKind::Hipped);
    let kinds: Vec<RoofEdgeKind> = roof
        .faces
        .iter()
        .flat_map(|f| {
            f.outline
                .as_ref()
                .map(|o| o.edge_kinds.clone())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == RoofEdgeKind::Valley).count(),
        2,
        "{kinds:?}"
    );
    let h = 6.0 + 5.0 * 30f64.to_radians().tan();
    assert!(
        (roof.ridge_height_m - h).abs() < 1e-9,
        "ridges at half the arm width"
    );
    // The same L as a gable roof (arm ends vertical): valleys, no hips.
    let mut spec = presets::hipped(&fp, 30.0, 30.0, 0.3);
    spec.edges[1] = EdgeRoof::gable();
    spec.edges[4] = EdgeRoof::gable();
    let b = building(fp, spec);
    let roof = generate(&b).expect("gable L");
    check_roof(&b, &roof).unwrap();
}

/// Orthogonal "histogram" footprints: many reflex corners and simultaneous events (heights
/// on a 1 m grid), random pitches, gable walls, raised eaves and overhangs.
#[test]
fn random_orthogonal_footprints_hold_every_invariant() {
    let mut rng = Rng(0x0047_0001);
    let mut stats = BTreeMap::new();
    for case in 0..cases(600) {
        let cols = 2 + rng.below(4) as usize;
        let mut fp: Vec<[f64; 2]> = Vec::new();
        let widths: Vec<f64> = (0..cols).map(|_| 3.0 + rng.below(6) as f64).collect();
        let heights: Vec<f64> = (0..cols).map(|_| 4.0 + rng.below(8) as f64).collect();
        let total: f64 = widths.iter().sum();
        fp.push([0.0, 0.0]);
        fp.push([total, 0.0]);
        let mut x = total;
        for i in (0..cols).rev() {
            fp.push([x, heights[i]]);
            x -= widths[i];
            fp.push([x, heights[i]]);
        }
        // Merge vertices of equal neighbouring heights (collinear) and duplicates.
        let pts: Vec<Vec2> = fp.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        let pts = pv_roof::clean(pts);
        let fp: Vec<[f64; 2]> = pts.iter().map(|p| [p.u, p.v]).collect();
        let n = fp.len();
        let uniform = rng.below(3) == 0;
        let edges = if uniform {
            let pitch = rng.range(15.0, 45.0);
            vec![EdgeRoof::sloped(pitch); n]
        } else {
            random_edges(&mut rng, n, 6, 10)
        };
        if edges.iter().all(|e| !e.is_sloped()) {
            continue;
        }
        let overhang_m = if rng.below(2) == 0 {
            0.0
        } else {
            rng.range(0.1, 0.5)
        };
        let b = building(fp, RoofSpec { edges, overhang_m });
        let roof = match generate(&b) {
            Ok(r) => r,
            // One pitch everywhere: every edge starts together, never refused.
            Err(pv_roof::RoofError::ReentrantCorner { .. }) if !uniform => {
                *stats.entry("refused".to_owned()).or_insert(0) += 1;
                continue;
            }
            Err(e) => panic!("case {case}: {e}\n{b:?}"),
        };
        check_roof(&b, &roof).unwrap_or_else(|e| panic!("case {case}: {e}\n{b:?}"));
        *stats.entry(format!("{:?}", roof.roof_kind)).or_insert(0) += 1;
    }
    eprintln!("{stats:?}");
}

/// Star-shaped footprints in general position.
#[test]
fn random_star_footprints_hold_every_invariant() {
    let mut rng = Rng(0x0057_A001);
    for case in 0..cases(600) {
        let n = 5 + rng.below(8) as usize;
        let fp: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * (i as f64 + rng.range(-0.3, 0.3)) / n as f64;
                let r = rng.range(5.0, 14.0);
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        if pv_model::roof::validate_footprint(&fp).is_err() {
            continue;
        }
        let edges = random_edges(&mut rng, n, 8, 12);
        if edges.iter().all(|e| !e.is_sloped()) {
            continue;
        }
        let overhang_m = if rng.below(2) == 0 {
            0.0
        } else {
            rng.range(0.05, 0.3)
        };
        let b = building(fp, RoofSpec { edges, overhang_m });
        let roof = match generate(&b) {
            Ok(r) => r,
            Err(pv_roof::RoofError::Overhang | pv_roof::RoofError::ReentrantCorner { .. }) => {
                continue
            }
            Err(e) => panic!("case {case}: {e}\n{b:?}"),
        };
        check_roof(&b, &roof).unwrap_or_else(|e| panic!("case {case}: {e}\n{b:?}"));
    }
}
