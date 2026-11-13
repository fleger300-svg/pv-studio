//! One-layer rail systems (K2 SingleRail on tile hooks, modules in portrait) and fixations
//! on every n-th rafter.

use pv_geo::Vec2;
use pv_layout::{
    build_structure, cross_rails, every_nth, fixations, Cell, CrossRailParams, GridSpec,
    JointRules, SupportKind, SupportLines,
};
use pv_model::{LayoutAnchor, RailLayers};

/// Portrait module 1.134 × 1.800 m, 20 mm gaps.
fn spec() -> GridSpec {
    GridSpec {
        footprint_u: 1.134,
        footprint_v: 1.8,
        gap_u: 0.02,
        gap_v: 0.02,
        block: None,
        row_pitch_v: None,
        anchor: LayoutAnchor::LowerLeft,
    }
}

fn one_layer() -> CrossRailParams {
    CrossRailParams {
        layers: RailLayers::One,
        per_column: 2,
        top_inset_m: 0.4,
        top_overhang_m: 0.05,
        ..CrossRailParams::k2_single_rail()
    }
}

/// `rows` × `cols` field, without `missing` (row, col) cells.
fn cells(cols: u32, rows: u32, missing: &[(u32, u32)]) -> Vec<Cell> {
    let s = spec();
    let mut out = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            if missing.contains(&(row, col)) {
                continue;
            }
            out.push(Cell {
                row,
                col,
                centre: Vec2::new(
                    f64::from(col) * s.pitch_u() + s.footprint_u / 2.0,
                    f64::from(row) * s.pitch_v() + s.footprint_v / 2.0,
                ),
            });
        }
    }
    out
}

#[test]
fn two_rails_per_row_carry_the_modules() {
    let cs = cells(5, 2, &[]);
    let st = build_structure(&cs, &spec(), &JointRules::default()).unwrap();
    let r = cross_rails(&cs, &spec(), &st, &one_layer());
    assert!(r.top.is_empty() && r.climbers.is_empty());
    assert_eq!(r.bottom.len(), 4);
    assert_eq!(r.bottom_modules, vec![5; 4]);
    // 5 modules + 4 gaps + 2 overhangs.
    let expected = 5.0 * 1.134 + 4.0 * 0.02 + 2.0 * 0.05;
    for rail in &r.bottom {
        assert!((rail.length() - expected).abs() < 1e-9);
        assert!(
            (rail.start.v - rail.end.v).abs() < 1e-12,
            "rails run along u"
        );
    }
    // Rails of row 0 at 0.40 m from the module edges.
    let mut vs: Vec<f64> = r.bottom.iter().map(|x| x.start.v).collect();
    vs.sort_by(f64::total_cmp);
    assert!((vs[0] - 0.4).abs() < 1e-9 && (vs[1] - 1.4).abs() < 1e-9);
}

#[test]
fn a_gap_in_a_row_splits_its_rails() {
    let cs = cells(5, 1, &[(0, 2)]);
    let st = build_structure(&cs, &spec(), &JointRules::default()).unwrap();
    let r = cross_rails(&cs, &spec(), &st, &one_layer());
    let mut carried = r.bottom_modules.clone();
    carried.sort_unstable();
    assert_eq!(carried, vec![2, 2, 2, 2]);
}

#[test]
fn hooks_on_every_nth_rafter() {
    let cs = cells(5, 1, &[]);
    let st = build_structure(&cs, &spec(), &JointRules::default()).unwrap();
    let r = cross_rails(&cs, &spec(), &st, &one_layer());
    let rafters = SupportLines {
        kind: SupportKind::Rafters,
        origin_m: 0.1,
        spacing_m: 0.6,
    };
    let all = fixations(&r, &rafters, 0.05, None, 1);
    let every2 = fixations(&r, &rafters, 0.05, None, 2);
    for (a, b) in all.positions.iter().zip(&every2.positions) {
        // Outer hooks kept, no gap above 2 rafter spacings.
        assert_eq!(a.first(), b.first());
        assert_eq!(a.last(), b.last());
        assert!(b.windows(2).all(|w| w[1] - w[0] <= 1.2 + 1e-9));
        assert!(b.len() < a.len());
    }
}

#[test]
fn every_nth_keeps_ends() {
    let p: Vec<f64> = (0..8).map(f64::from).collect();
    assert_eq!(every_nth(p.clone(), 1), p);
    assert_eq!(every_nth(p.clone(), 2), vec![0.0, 2.0, 4.0, 6.0, 7.0]);
    assert_eq!(every_nth(p, 3), vec![0.0, 3.0, 6.0, 7.0]);
    assert_eq!(every_nth(vec![0.0, 1.0], 3), vec![0.0, 1.0]);
}
