//! L1: roof outlines, setbacks and obstacles.

use pv_geo::{OrientedRect, Polygon, Vec2};
use pv_layout::{
    BuildableArea, EdgeKind, Obstacle, ObstacleKind, Rejection, RoofOutline, SetbackRules,
};

fn p(u: f64, v: f64) -> Vec2 {
    Vec2::new(u, v)
}

/// L-shaped face 12 × 8 m with the upper-right 5 × 4 m cut away by a higher wall:
/// eave at v = 0, ridge at v = 8 (left part), abutment along the step.
fn l_roof() -> RoofOutline {
    RoofOutline::new(
        vec![
            p(0.0, 0.0),
            p(12.0, 0.0),
            p(12.0, 4.0),
            p(7.0, 4.0),
            p(7.0, 8.0),
            p(0.0, 8.0),
        ],
        vec![
            EdgeKind::Eave,
            EdgeKind::Verge,
            EdgeKind::Abutment,
            EdgeKind::Abutment,
            EdgeKind::Ridge,
            EdgeKind::Verge,
        ],
        vec![],
    )
    .unwrap()
}

fn rules() -> SetbackRules {
    SetbackRules::uniform(0.3, 0.2)
        .with_edge(EdgeKind::Eave, 0.2)
        .with_edge(EdgeKind::Ridge, 0.4)
        .with_edge(EdgeKind::Abutment, 0.5)
        .with_obstacle(ObstacleKind::Chimney, 0.5)
}

fn module_at(lower_left: Vec2) -> OrientedRect {
    // 1.134 × 1.800 portrait module.
    OrientedRect::from_corners(lower_left, lower_left + p(1.134, 1.8))
}

#[test]
fn setbacks_depend_on_edge_kind() {
    let area = BuildableArea::new(l_roof(), &rules());
    // 0.25 m above the eave: fine (eave 0.2 m), 0.25 m from the left verge: too close (0.3 m).
    assert!(area.accepts(&module_at(p(0.3, 0.25))));
    match area.check(&module_at(p(0.25, 0.25))) {
        Some(Rejection::TooCloseToEdge {
            kind: EdgeKind::Verge,
            required_m,
            ..
        }) => assert_eq!(required_m, 0.3),
        other => panic!("expected verge setback, got {other:?}"),
    }
    // Under the ridge: top at 7.6 m keeps exactly 0.4 m.
    assert!(area.accepts(&module_at(p(1.0, 5.8))));
    assert!(matches!(
        area.check(&module_at(p(1.0, 5.9))),
        Some(Rejection::TooCloseToEdge {
            kind: EdgeKind::Ridge,
            ..
        })
    ));
}

#[test]
fn concave_corner_is_respected() {
    let area = BuildableArea::new(l_roof(), &rules());
    // Module just left of the step (u ≤ 6.5 keeps 0.5 m to the abutment at u = 7).
    assert!(area.accepts(&module_at(p(5.366, 4.5))));
    // 1 cm further right: too close to the vertical abutment.
    assert!(matches!(
        area.check(&module_at(p(5.376, 4.5))),
        Some(Rejection::TooCloseToEdge {
            kind: EdgeKind::Abutment,
            ..
        })
    ));
    // Straddling the reflex corner (7, 4): outside the roof even though corners are inside
    // the bounding box.
    assert_eq!(
        area.check(&module_at(p(6.5, 3.0))),
        Some(Rejection::OutsideRoof)
    );
}

#[test]
fn obstacles_block_and_keep_clearance() {
    let chimney = Obstacle {
        name: "chimney".into(),
        kind: ObstacleKind::Chimney,
        footprint: Polygon::rectangle(3.0, 2.0, 3.6, 2.6),
        height_m: 1.2,
        clearance_m: None,
    };
    let skylight = Obstacle {
        name: "skylight".into(),
        kind: ObstacleKind::Skylight,
        footprint: Polygon::rectangle(9.0, 1.0, 10.0, 2.0),
        height_m: 0.1,
        clearance_m: Some(0.0),
    };
    let roof = l_roof()
        .with_obstacle(chimney)
        .unwrap()
        .with_obstacle(skylight)
        .unwrap();
    let area = BuildableArea::new(roof, &rules());
    // Overlap.
    assert!(matches!(
        area.check(&module_at(p(2.5, 1.5))),
        Some(Rejection::OverlapsObstacle { obstacle: 0, .. })
    ));
    // 0.4 m left of the chimney: chimney clearance is 0.5 m.
    match area.check(&module_at(p(3.0 - 0.4 - 1.134, 1.5))) {
        Some(Rejection::TooCloseToObstacle {
            obstacle: 0,
            required_m,
            ..
        }) => assert_eq!(required_m, 0.5),
        other => panic!("expected chimney clearance, got {other:?}"),
    }
    assert!(area.accepts(&module_at(p(3.0 - 0.5 - 1.134, 1.5))));
    // Skylight has an explicit zero clearance: touching is allowed, overlapping is not.
    assert!(area.accepts(&module_at(p(10.0, 0.5))));
    assert!(!area.accepts(&module_at(p(9.9, 0.5))));
}

#[test]
fn rotated_footprints_are_checked_exactly() {
    let area = BuildableArea::new(
        RoofOutline::flat(10.0, 10.0).unwrap(),
        &SetbackRules::uniform(1.0, 0.0),
    );
    // A 2 × 1 m module turned 45°: its corner reaches 1.0607 m from the centre along u, so
    // with a 1 m setback the centre must be at u ≥ 2.0607.
    let r = OrientedRect::axis_aligned(p(1.0 + 1.0607, 5.0), 2.0, 1.0)
        .rotated(std::f64::consts::FRAC_PI_4);
    assert!(area.accepts(&r));
    let r = OrientedRect::axis_aligned(p(1.0 + 1.05, 5.0), 2.0, 1.0)
        .rotated(std::f64::consts::FRAC_PI_4);
    assert!(matches!(
        area.check(&r),
        Some(Rejection::TooCloseToEdge {
            kind: EdgeKind::Parapet,
            ..
        })
    ));
}

/// Independent oracle: sample the boundary every 5 mm and measure distances to the
/// rectangle directly; every footprint the engine accepts must pass it.
#[test]
fn accepted_footprints_pass_an_independent_check() {
    let roof = l_roof()
        .with_obstacle(Obstacle {
            name: "vent".into(),
            kind: ObstacleKind::Vent,
            footprint: Polygon::new(vec![p(4.0, 1.0), p(4.6, 1.2), p(4.3, 1.8)]).unwrap(),
            height_m: 0.5,
            clearance_m: None,
        })
        .unwrap();
    let rules = rules();
    let area = BuildableArea::new(roof.clone(), &rules);
    let dist = |r: &OrientedRect, q: Vec2| -> f64 {
        if r.contains_point(q) {
            0.0
        } else {
            r.edges()
                .iter()
                .map(|e| e.distance_to_point(q))
                .fold(f64::INFINITY, f64::min)
        }
    };
    let mut accepted = 0;
    for i in 0..60 {
        for j in 0..40 {
            for angle in [0.0, 0.3] {
                let r = OrientedRect::axis_aligned(
                    p(0.2 * f64::from(i), 0.2 * f64::from(j)),
                    1.134,
                    1.8,
                )
                .rotated(angle);
                if !area.accepts(&r) {
                    continue;
                }
                accepted += 1;
                assert!(r
                    .corners()
                    .iter()
                    .all(|&c| roof.boundary().contains_point(c)));
                for (k, edge) in roof.boundary().edges().enumerate() {
                    let need = rules.edge(roof.edge_kind(k));
                    let n = (edge.length() / 0.005).ceil() as usize;
                    for s in 0..=n {
                        let q = edge.a + edge.direction() * (s as f64 / n as f64);
                        assert!(
                            dist(&r, q) >= need - 1e-3,
                            "edge {k} too close at {q:?} for {r:?}"
                        );
                    }
                }
                let vent = &roof.obstacles()[0].footprint;
                for e in vent.edges() {
                    for s in 0..=100 {
                        let q = e.a + e.direction() * (f64::from(s) / 100.0);
                        assert!(dist(&r, q) >= rules.obstacle(ObstacleKind::Vent) - 1e-3);
                    }
                }
            }
        }
    }
    assert!(accepted > 50, "sweep accepted only {accepted} footprints");
}

#[test]
fn outline_round_trips_through_json() {
    let json = r#"{
        "vertices": [{"u":0,"v":0},{"u":0,"v":4},{"u":6,"v":4},{"u":6,"v":0}],
        "edge_kinds": ["verge","ridge","verge","eave"],
        "obstacles": [{"name":"c","kind":"chimney","footprint":{"vertices":[{"u":1,"v":1},{"u":2,"v":1},{"u":2,"v":2}]}}]
    }"#;
    let roof: RoofOutline = serde_json::from_str(json).unwrap();
    assert!((roof.area_m2() - 24.0).abs() < 1e-12);
    // Clockwise input normalised; the eave is still the edge at v = 0.
    let eave = (0..4)
        .find(|&i| roof.edge_kind(i) == EdgeKind::Eave)
        .unwrap();
    assert_eq!(roof.boundary().edge(eave).a.v, 0.0);
    assert_eq!(roof.obstacles().len(), 1);
    assert!(serde_json::from_str::<RoofOutline>(
        r#"{"vertices":[{"u":0,"v":0}],"edge_kinds":["eave"]}"#
    )
    .is_err());
}
