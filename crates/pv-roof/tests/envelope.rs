//! E2.2: convex footprints (lower envelope of the edge planes).

mod common;

use common::*;
use pv_geo::Vec2;
use pv_model::{EdgeRoof, RoofEdgeKind, RoofFace, RoofKind, RoofSpec, StressPreset};
use pv_roof::{classify, generate, presets, walls};

fn kinds_of(f: &RoofFace) -> Vec<RoofEdgeKind> {
    f.outline
        .as_ref()
        .map_or_else(Vec::new, |o| o.edge_kinds.clone())
}

#[test]
fn hipped_rectangle_matches_the_closed_form() {
    let (l, w, pitch) = (16.0, 10.0, 35.0);
    let b = building(rect(l, w), presets::hipped(&rect(l, w), pitch, pitch, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    assert_eq!(roof.roof_kind, RoofKind::Hipped);
    assert_eq!(roof.faces.len(), 4);
    let t = pitch.to_radians().tan();
    assert!((roof.ridge_height_m - (6.0 + w / 2.0 * t)).abs() < 1e-12);
    // South trapezoid: eave L, ridge L − W, slope length (W/2)/cos θ.
    let south = &roof.faces[0];
    assert_eq!(south.azimuth_deg, 180.0);
    assert!((south.width_m - l).abs() < 1e-12);
    assert!((south.height_m - w / 2.0 / pitch.to_radians().cos()).abs() < 1e-12);
    let o = south.outline.as_ref().expect("trapezoid");
    let ridge: Vec<[f64; 2]> = o
        .vertices_uv
        .iter()
        .copied()
        .filter(|p| p[1] > 1.0)
        .collect();
    assert!((ridge[0][0] - ridge[1][0]).abs() - (l - w) < 1e-9);
    assert_eq!(
        kinds_of(south),
        vec![
            RoofEdgeKind::Eave,
            RoofEdgeKind::Hip,
            RoofEdgeKind::Ridge,
            RoofEdgeKind::Hip
        ]
    );
    // East triangle.
    let east = &roof.faces[1];
    assert_eq!(east.azimuth_deg, 90.0);
    assert_eq!(east.outline.as_ref().unwrap().vertices_uv.len(), 3);
    assert_eq!(
        kinds_of(east),
        vec![RoofEdgeKind::Eave, RoofEdgeKind::Hip, RoofEdgeKind::Hip]
    );
    assert_eq!(roof.size_m, [l, w]);
}

#[test]
fn square_hipped_roof_is_a_pyramid() {
    let fp = rect(9.0, 9.0);
    let b = building(fp.clone(), presets::hipped(&fp, 30.0, 30.0, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    assert_eq!(roof.faces.len(), 4);
    for f in &roof.faces {
        assert_eq!(f.outline.as_ref().unwrap().vertices_uv.len(), 3);
        assert!(!kinds_of(f).contains(&RoofEdgeKind::Ridge));
    }
}

#[test]
fn steeper_hip_ends_shorten_the_ridge() {
    let (l, w, p, pe) = (20.0, 10.0, 30.0_f64, 50.0_f64);
    let fp = rect(l, w);
    let b = building(fp.clone(), presets::hipped(&fp, p, pe, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    // Each hip end reaches the ridge height H = W/2·tan p at a distance H / tan pe.
    let inset = w / 2.0 * p.to_radians().tan() / pe.to_radians().tan();
    let south = &roof.plan_faces[0];
    let top: Vec<Vec2> = south
        .polygon
        .iter()
        .copied()
        .filter(|q| q.v > -1.0)
        .collect();
    let ridge_len = (top[0].u - top[1].u).abs();
    assert!((ridge_len - (l - 2.0 * inset)).abs() < 1e-9, "{ridge_len}");
}

#[test]
fn half_hipped_ends_start_above_the_eave() {
    let fp = rect(14.0, 9.0);
    let b = building(fp.clone(), presets::half_hipped(&fp, 40.0, 55.0, 8.0, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    assert_eq!(roof.roof_kind, RoofKind::Hipped);
    let east = roof
        .faces
        .iter()
        .find(|f| f.azimuth_deg == 90.0)
        .expect("east end");
    assert!((east.origin[2] - (2.0 + 8.0)).abs() < 1e-9, "raised eave");
    // The east wall: eave, up the verges to the raised eave, flat under it, down again.
    let face_refs: Vec<&RoofFace> = roof.faces.iter().collect();
    let ws = walls(&roof.building(&b), &face_refs);
    let east_wall = &ws[1];
    let zs: Vec<f64> = east_wall.profile.iter().map(|p| p[1]).collect();
    assert!((zs[0] - 8.0).abs() < 1e-9 && (zs[zs.len() - 1] - 8.0).abs() < 1e-9);
    assert!(zs.iter().any(|z| (z - 10.0).abs() < 1e-9));
}

#[test]
fn monopitch_and_flat() {
    let fp = rect(12.0, 6.0);
    let b = building(fp.clone(), presets::monopitch(&fp, 0, 15.0, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    assert_eq!(roof.roof_kind, RoofKind::Monopitch);
    assert_eq!(
        kinds_of(&roof.faces[0]),
        vec![
            RoofEdgeKind::Eave,
            RoofEdgeKind::Verge,
            RoofEdgeKind::Verge,
            RoofEdgeKind::Verge
        ]
    );
    let b = building(fp.clone(), presets::flat(&fp, 0.0));
    let roof = generate(&b).expect("roof");
    check_roof(&b, &roof).unwrap();
    assert_eq!(roof.roof_kind, RoofKind::Flat);
    let f = &roof.faces[0];
    assert_eq!(
        (f.tilt_deg, f.azimuth_deg, f.outline.is_none()),
        (0.0, 180.0, true)
    );
    assert_eq!(f.name, "Flat roof");
}

#[test]
fn overhang_extends_the_faces_and_keeps_the_planes() {
    let fp = rect(12.0, 8.0);
    let plain = building(fp.clone(), presets::gable(&fp, 30.0, 0.0));
    let over = building(fp.clone(), presets::gable(&fp, 30.0, 0.5));
    let (r0, r1) = (generate(&plain).unwrap(), generate(&over).unwrap());
    check_roof(&over, &r1).unwrap();
    assert!(
        (r0.ridge_height_m - r1.ridge_height_m).abs() < 1e-12,
        "same ridge"
    );
    let (f0, f1) = (&r0.faces[0], &r1.faces[0]);
    assert!((f1.width_m - (f0.width_m + 1.0)).abs() < 1e-12);
    assert!((f1.height_m - (f0.height_m + 0.5 / 30f64.to_radians().cos())).abs() < 1e-12);
    // Same plane: f0's origin lies on f1's plane.
    assert!(f1.frame().to_local(f0.origin).2.abs() < 1e-12);
    // The wall under the eave reaches the roof above the wall line, not the eave.
    let refs: Vec<&RoofFace> = r1.faces.iter().collect();
    let ws = walls(&r1.building(&over), &refs);
    assert!((ws[0].profile[0][1] - (2.0 + 6.0)).abs() < 1e-9);
}

/// The residential stress preset's gable roofs, placed by hand, are exactly what the
/// generator makes of their footprint: the generator follows the model's frame convention.
#[test]
fn regenerates_the_hand_made_gable_roofs_of_the_presets() {
    let p = StressPreset::Residential1k.build();
    for b in &p.buildings {
        let mut g = b.clone();
        g.roof = Some(presets::gable(&b.footprint, 30.0, 0.0));
        let roof = generate(&g).expect("roof");
        check_roof(&g, &roof).unwrap();
        let hand: Vec<&RoofFace> = p
            .roof_faces
            .iter()
            .filter(|f| f.building_id == b.id)
            .collect();
        assert_eq!(roof.faces.len(), hand.len());
        for (gen, h) in roof.faces.iter().zip(&hand) {
            assert_eq!(gen.name, h.name);
            assert_eq!(gen.tilt_deg, h.tilt_deg);
            assert_eq!(gen.azimuth_deg, h.azimuth_deg);
            assert!(gen.outline.is_none(), "default rectangle");
            for i in 0..3 {
                assert!(
                    (gen.origin[i] - h.origin[i]).abs() < 1e-9,
                    "{:?} {:?}",
                    gen.origin,
                    h.origin
                );
            }
            assert!((gen.width_m - h.width_m).abs() < 1e-9);
            assert!((gen.height_m - h.height_m).abs() < 1e-9);
        }
        assert!((roof.ridge_height_m - b.ridge_height_m).abs() < 1e-9);
        assert_eq!(roof.roof_kind, b.roof_kind);
        assert_eq!(roof.size_m, b.size_m);
    }
}

/// The flat industrial roof, the same way.
#[test]
fn regenerates_the_flat_industrial_roof() {
    let p = StressPreset::Industrial10k.build();
    let b = &p.buildings[0];
    let hand = &p.roof_faces[0];
    let mut g = b.clone();
    g.roof = Some(presets::flat(&b.footprint, 0.0));
    let roof = generate(&g).expect("roof");
    let f = &roof.faces[0];
    assert_eq!(
        (f.tilt_deg, f.azimuth_deg),
        (hand.tilt_deg, hand.azimuth_deg)
    );
    for i in 0..3 {
        assert!((f.origin[i] - hand.origin[i]).abs() < 1e-9);
    }
    assert!((f.width_m - hand.width_m).abs() < 1e-9 && (f.height_m - hand.height_m).abs() < 1e-9);
}

#[test]
fn gable_walls_have_triangular_ends() {
    let p = StressPreset::Residential1k.build();
    let b = &p.buildings[0];
    let faces: Vec<&RoofFace> = p
        .roof_faces
        .iter()
        .filter(|f| f.building_id == b.id)
        .collect();
    let ws = walls(b, &faces);
    assert_eq!(ws.len(), 4);
    let depth = b.size_m[1];
    // East wall (edge 1): eave, ridge at mid-depth, eave.
    let east = &ws[1].profile;
    assert_eq!(east.len(), 3, "{east:?}");
    assert!((east[1][0] - depth / 2.0).abs() < 1e-9);
    assert!((east[1][1] - b.ridge_height_m).abs() < 1e-9);
    // South wall: flat at the eave.
    assert!(ws[0]
        .profile
        .iter()
        .all(|q| (q[1] - b.eave_height_m).abs() < 1e-9));
}

/// 1 000 random convex footprints with random pitches (some vertical), raised eaves and
/// overhangs: every invariant holds, and the roof is the lower envelope of the planes.
#[test]
fn random_convex_roofs_hold_every_invariant() {
    let mut rng = Rng(0x00C0_FFEE);
    let mut kinds = std::collections::BTreeMap::new();
    for case in 0..1000 {
        let fp = convex_footprint(&mut rng);
        let edges = (0..fp.len())
            .map(|_| {
                if rng.below(5) == 0 {
                    EdgeRoof::gable()
                } else {
                    EdgeRoof {
                        pitch_deg: rng.range(8.0, 60.0),
                        eave_height_m: (rng.below(10) == 0).then(|| rng.range(6.0, 8.0)),
                    }
                }
            })
            .collect();
        let overhang_m = if rng.below(3) == 0 {
            0.0
        } else {
            rng.range(0.1, 0.8)
        };
        let b = building(fp, RoofSpec { edges, overhang_m });
        let roof = generate(&b).unwrap_or_else(|e| panic!("case {case}: {e}"));
        check_roof(&b, &roof).unwrap_or_else(|e| panic!("case {case}: {e}\n{b:?}"));
        // Envelope: each face is the lowest plane at its centroid.
        let input = &roof.input;
        for pf in &roof.plan_faces {
            let c =
                pf.polygon.iter().fold(Vec2::ZERO, |s, &p| s + p) * (1.0 / pf.polygon.len() as f64);
            let z = classify::face_z(input, pf, c);
            for pl in input.planes.iter().filter(|pl| pl.is_sloped()) {
                assert!(z <= pl.z(c) + 1e-9, "case {case}: a plane is lower");
            }
        }
        assert_eq!(generate(&b).unwrap(), roof, "deterministic");
        *kinds.entry(format!("{:?}", roof.roof_kind)).or_insert(0) += 1;
    }
    eprintln!("{kinds:?}");
}
