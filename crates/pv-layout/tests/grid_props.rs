//! L2 invariants: whatever the roof and the grid parameters, a fill never places a module
//! outside the region or on top of another, never does worse than the unshifted lattice, and
//! the `pv-model` bridge stays consistent with `ArrayGrid::cell_centre`.

use pv_geo::{OrientedRect, Polygon, Vec2};
use pv_layout::model::{grid_spec, layout_array, layout_east_west};
use pv_layout::{
    fill, place_fixed, BuildableArea, EdgeKind, GridSpec, Obstacle, ObstacleKind, Region,
    RoofOutline, Search, SetbackRules,
};
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, BlockParams, BuildingId, LayoutAnchor, Orientation,
    PanelArray, RoofFace, RoofFaceId,
};

fn p(u: f64, v: f64) -> Vec2 {
    Vec2::new(u, v)
}

fn l_roof_with_obstacles() -> RoofOutline {
    RoofOutline::new(
        vec![
            p(0.0, 0.0),
            p(14.0, 0.0),
            p(14.0, 4.5),
            p(8.0, 4.5),
            p(8.0, 9.0),
            p(0.0, 9.0),
        ],
        vec![
            EdgeKind::Eave,
            EdgeKind::Verge,
            EdgeKind::Abutment,
            EdgeKind::Abutment,
            EdgeKind::Ridge,
            EdgeKind::Verge,
        ],
        vec![
            Obstacle {
                name: "chimney".into(),
                kind: ObstacleKind::Chimney,
                footprint: Polygon::rectangle(3.0, 5.0, 3.8, 5.8),
                height_m: 1.2,
                clearance_m: None,
            },
            Obstacle {
                name: "vent".into(),
                kind: ObstacleKind::Vent,
                footprint: Polygon::new(vec![p(10.0, 1.5), p(10.6, 1.7), p(10.2, 2.3)]).unwrap(),
                height_m: 0.4,
                clearance_m: None,
            },
        ],
    )
    .unwrap()
}

fn specs() -> Vec<GridSpec> {
    let base = GridSpec {
        footprint_u: 1.134,
        footprint_v: 1.762,
        gap_u: 0.02,
        gap_v: 0.02,
        block: None,
        row_pitch_v: None,
        anchor: LayoutAnchor::LowerLeft,
    };
    let blocks = Some(BlockParams {
        max_cols: 3,
        max_rows: 2,
        gap_u_m: 0.10,
        gap_v_m: 0.15,
    });
    vec![
        base,
        GridSpec {
            footprint_u: 1.762,
            footprint_v: 1.134,
            ..base
        },
        GridSpec {
            block: blocks,
            ..base
        },
        GridSpec {
            block: blocks,
            anchor: LayoutAnchor::UpperRight,
            ..base
        },
        GridSpec {
            anchor: LayoutAnchor::Centre,
            ..base
        },
        // Tilted rows on a flat part: rows further apart than the footprint.
        GridSpec {
            footprint_v: 1.6,
            row_pitch_v: Some(2.6),
            ..base
        },
    ]
}

#[test]
fn fills_are_valid_and_never_worse_than_unshifted() {
    let rules = SetbackRules::uniform(0.3, 0.3)
        .with_edge(EdgeKind::Eave, 0.2)
        .with_edge(EdgeKind::Abutment, 0.5);
    let area = BuildableArea::new(l_roof_with_obstacles(), &rules);
    let region = Region {
        area: &area,
        boundary: None,
    };
    for spec in specs() {
        let filled = fill(region, spec, Search::default());
        assert!(
            filled.count() > 10,
            "{spec:?}: only {} modules",
            filled.count()
        );
        let rects: Vec<OrientedRect> = filled.cells.iter().map(|c| filled.footprint(c)).collect();
        for (i, r) in rects.iter().enumerate() {
            assert!(
                area.accepts(r),
                "{spec:?}: module {i} rejected: {:?}",
                area.check(r)
            );
            for o in &rects[i + 1..] {
                assert!(!r.overlaps(o), "{spec:?}: overlapping modules");
            }
        }
        // Rows respect the row pitch.
        if let Some(pitch) = spec.row_pitch_v {
            let mut vs: Vec<f64> = filled.cells.iter().map(|c| c.centre.v).collect();
            vs.sort_by(f64::total_cmp);
            vs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            for w in vs.windows(2) {
                assert!(w[1] - w[0] >= pitch - 1e-9);
            }
        }
        // The unshifted lattice flush with the region's lower-left corner.
        let (lo, hi) = region.bounds();
        let unshifted = place_fixed(
            region,
            GridSpec {
                anchor: LayoutAnchor::LowerLeft,
                ..spec
            },
            lo + p(spec.footprint_u / 2.0, spec.footprint_v / 2.0),
            ((hi.v - lo.v) / spec.pitch_v()) as u32 + 1,
            ((hi.u - lo.u) / spec.pitch_u()) as u32 + 1,
        );
        if spec.anchor == LayoutAnchor::LowerLeft {
            assert!(
                filled.count() >= unshifted.count(),
                "{spec:?}: fill {} < unshifted {}",
                filled.count(),
                unshifted.count()
            );
        }
    }
}

fn face(tilt: f64) -> RoofFace {
    RoofFace {
        id: RoofFaceId::new(),
        building_id: BuildingId::new(),
        name: "f".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: tilt,
        azimuth_deg: 180.0,
        width_m: 12.0,
        height_m: 7.0,
        outline: None,
    }
}

fn array(face: &RoofFace, azimuth_offset: f64, tilt: f64, layout: ArrayLayoutParams) -> PanelArray {
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: face.id,
        module_length_m: 1.762,
        module_width_m: 1.134,
        orientation: Orientation::Portrait,
        tilt_on_roof_deg: tilt,
        azimuth_offset_deg: azimuth_offset,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        layout,
    }
}

#[test]
fn model_bridge_matches_array_grid() {
    let f = face(30.0);
    let layout = ArrayLayoutParams {
        module_gap_u_m: 0.02,
        module_gap_v_m: 0.02,
        block: Some(BlockParams {
            max_cols: 4,
            max_rows: 2,
            gap_u_m: 0.1,
            gap_v_m: 0.12,
        }),
        edge_setback_m: 0.3,
        anchor: LayoutAnchor::UpperRight,
        ..ArrayLayoutParams::default()
    };
    let a = array(&f, 0.0, 0.0, layout);
    let l = layout_array(&f, &a, Search::default()).unwrap();
    assert!(!l.cells.is_empty());
    for c in &l.cells {
        let (u, v) = l.grid.cell_centre(c.row, c.col);
        assert!(
            (u - c.centre.u).abs() < 1e-9 && (v - c.centre.v).abs() < 1e-9,
            "cell {c:?} vs grid ({u}, {v})"
        );
    }
    let instances = l.module_instances(a.id, 7);
    assert_eq!(instances.first().map(|m| m.id.0), Some(7));
    assert_eq!(instances.len(), l.cells.len());
    // Footprints from the bridge equal the 3D view's (PanelArray::half_footprint).
    let (hu, hv) = a.half_footprint();
    let s = grid_spec(&a);
    assert_eq!((s.footprint_u, s.footprint_v), (2.0 * hu, 2.0 * hv));
}

#[test]
fn east_west_through_the_model() {
    let f = face(0.0);
    let layout = ArrayLayoutParams {
        module_gap_u_m: 0.08,
        module_gap_v_m: 0.038,
        edge_setback_m: 0.6,
        row_pitch_m: Some(2.46),
        ..ArrayLayoutParams::default()
    };
    let west = array(&f, 90.0, 8.6, layout.clone());
    let east = array(&f, -90.0, 8.6, layout);
    let (w, e) = layout_east_west(&f, &west, &east, Search::default()).unwrap();
    assert_eq!(w.cells.len(), e.cells.len());
    // 10.8 m usable / 2.46 m pairs, 5.8 m / 1.80 m rows.
    assert_eq!(w.cells.len(), 4 * 3);
    let area = BuildableArea::new(
        RoofOutline::from_face(&f).unwrap(),
        &SetbackRules::uniform(0.6, 0.0),
    );
    let (hu, hv) = west.half_footprint();
    for (a, b) in w.cells.iter().zip(&e.cells) {
        assert!(a.centre.u < b.centre.u, "+90° module on the −u side");
        for c in [a, b] {
            assert!(area.accepts(&OrientedRect::axis_aligned(c.centre, 2.0 * hu, 2.0 * hv)));
        }
        let (gu, gv) = w.grid.cell_centre(a.row, a.col);
        assert!((gu - a.centre.u).abs() < 1e-9 && (gv - a.centre.v).abs() < 1e-9);
    }
    // Wrong offsets are refused.
    assert!(layout_east_west(&f, &east, &west, Search::default()).is_err());
}
