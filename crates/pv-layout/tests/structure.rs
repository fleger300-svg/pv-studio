//! L3: mounting blocks, expansion joints and mechanical links.

use std::collections::BTreeMap;

use pv_geo::Vec2;
use pv_layout::model::{grid_spec, layout_array, structure};
use pv_layout::{build_structure, Cell, GridSpec, JointRules, JointSource, Search, Structure};
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, BlockParams, BuildingId, ExpansionJointSpec, JointAxis,
    LayoutAnchor, ModuleState, Orientation, PanelArray, RoofFace, RoofFaceId,
};

fn face() -> RoofFace {
    RoofFace {
        id: RoofFaceId::new(),
        building_id: BuildingId::new(),
        name: "f".into(),
        origin: [0.0, 0.0, 0.0],
        tilt_deg: 19.0,
        azimuth_deg: 180.0,
        width_m: 40.0,
        height_m: 12.0,
        outline: None,
        support_grid: None,
    }
}

/// Project A (K2 SingleRail cross system): landscape 1800 × 1134, Dm 10 mm, Dc 20 mm,
/// blocks of 2 × 2 with 0.08 / 0.13 m between blocks.
fn ref_a_array(f: &RoofFace, cols: u32, rows: u32) -> PanelArray {
    let (u0, v0) = (1.0, 1.0);
    let spec_probe = GridSpec {
        footprint_u: 1.8,
        footprint_v: 1.134,
        gap_u: 0.01,
        gap_v: 0.02,
        block: Some(BlockParams {
            max_cols: 2,
            max_rows: 2,
            gap_u_m: 0.08,
            gap_v_m: 0.13,
        }),
        row_pitch_v: None,
        anchor: LayoutAnchor::LowerLeft,
    };
    let (w, l) = spec_probe.extent(cols, rows);
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: f.id,
        module_length_m: 1.8,
        module_width_m: 1.134,
        orientation: Orientation::Landscape,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        layout: ArrayLayoutParams {
            boundary_uv: vec![
                [u0, v0],
                [u0 + w + 0.001, v0],
                [u0 + w + 0.001, v0 + l + 0.001],
                [u0, v0 + l + 0.001],
            ],
            module_gap_u_m: 0.01,
            module_gap_v_m: 0.02,
            block: spec_probe.block,
            ..ArrayLayoutParams::default()
        },
    }
}

fn block_sizes(s: &Structure) -> Vec<usize> {
    let mut v: Vec<usize> = s.blocks.iter().map(|b| b.cells.len()).collect();
    v.sort_unstable();
    v
}

/// The 8 fields of project A, with K2's block counts per roof (blocks are numbered 1…n per
/// roof in the report) and the one module K2 removed in roof-1 field 2 (block 6: « (2 x 2) - 1 »).
#[test]
fn ref_a_block_structure_matches_k2() {
    let f = face();
    // (roof, cols, rows, remove one module in lattice block (1, 1)?)
    let fields = [
        ("roof-1", 4, 4, false),
        ("roof-1", 5, 4, true),
        ("roof-2", 5, 4, false),
        ("roof-2", 3, 2, false),
        ("roof-3", 5, 4, false),
        ("roof-3", 3, 2, false),
        ("roof-4", 3, 3, false),
        ("roof-4", 5, 4, false),
    ];
    let mut blocks_per_roof: BTreeMap<&str, usize> = BTreeMap::new();
    for (roof, cols, rows, remove) in fields {
        let a = ref_a_array(&f, cols, rows);
        let layout = layout_array(&f, &a, Search::default()).unwrap();
        let mut modules = layout.module_instances(a.id, 0);
        if remove {
            // Top row (rows 2–3), middle lattice block (cols 2–3): K2 block 6.
            let m = modules
                .iter_mut()
                .find(|m| m.row == 3 && m.col == 3)
                .unwrap();
            m.state = ModuleState::Removed;
        }
        let (s, ids) = structure(&a, &modules).unwrap();
        assert_eq!(
            ids.len(),
            modules
                .iter()
                .filter(|m| m.state == ModuleState::Active)
                .count()
        );
        *blocks_per_roof.entry(roof).or_default() += s.blocks.len();
        assert!(s.joints.is_empty(), "no thermal rule, no manual joint");
        if remove {
            assert_eq!(
                block_sizes(&s),
                vec![2, 2, 3, 4, 4, 4],
                "{roof} {cols}×{rows}"
            );
            // 2×2 → 4 links, L of 3 → 2, 1×2 → 1.
            assert_eq!(s.links.len(), 4 + 4 + 4 + 2 + 1 + 1);
        }
        assert!(s.links.iter().all(|l| !l.crosses_expansion_joint));
    }
    let expected = [("roof-1", 10), ("roof-2", 8), ("roof-3", 8), ("roof-4", 10)];
    for (roof, n) in expected {
        assert_eq!(
            blocks_per_roof[roof], n,
            "{roof}: K2 numbers its blocks 1…{n}"
        );
    }
}

/// Project C roof 1: 6 landscape modules (1757 × 1134, Dm 10 mm) on one continuous 10.12 m
/// rail (K2 joins rail lengths with rail connectors, no expansion joint).
fn ref_c_cells() -> (Vec<Cell>, GridSpec) {
    let spec = GridSpec {
        footprint_u: 1.757,
        footprint_v: 1.134,
        gap_u: 0.01,
        gap_v: 0.02,
        block: None,
        row_pitch_v: None,
        anchor: LayoutAnchor::LowerLeft,
    };
    let cells = (0..6)
        .map(|c| Cell {
            row: 0,
            col: c,
            centre: Vec2::new(1.0 + spec.col_offset(c), 1.0),
        })
        .collect();
    (cells, spec)
}

#[test]
fn ref_c_single_rail_and_thermal_split() {
    let (cells, spec) = ref_c_cells();
    let s = build_structure(&cells, &spec, &JointRules::default()).unwrap();
    assert_eq!(s.blocks.len(), 1);
    assert_eq!(s.links.len(), 5);
    let (w, _) = s.blocks[0].size();
    assert!((w - 10.592).abs() < 1e-9, "6 × 1.757 + 5 × 0.01");

    // With a 6 m thermal rule: two blocks of 3 modules (5.291 m), one joint in the middle gap.
    let rules = JointRules {
        max_length_u: Some(6.0),
        ..JointRules::default()
    };
    let s = build_structure(&cells, &spec, &rules).unwrap();
    assert_eq!(block_sizes(&s), vec![3, 3]);
    assert_eq!(s.joints.len(), 1);
    let j = s.joints[0];
    assert_eq!(
        (j.axis, j.source),
        (JointAxis::ConstantU, JointSource::Thermal)
    );
    let mid = (cells[2].centre.u + cells[3].centre.u) / 2.0;
    assert!((j.position_m - mid).abs() < 1e-12);
    assert_eq!(
        s.links.iter().filter(|l| l.crosses_expansion_joint).count(),
        1
    );
    assert!(s.blocks.iter().all(|b| b.size().0 <= 6.0));

    // A rule shorter than one module is reported.
    let too_short = JointRules {
        max_length_u: Some(1.5),
        ..JointRules::default()
    };
    assert!(build_structure(&cells, &spec, &too_short).is_err());
}

#[test]
fn holes_and_manual_joints() {
    let (mut cells, spec) = ref_c_cells();
    // Manual joint between modules 1 and 2 (line u = middle of their gap).
    let x = (cells[1].centre.u + cells[2].centre.u) / 2.0;
    let rules = JointRules {
        manual: vec![ExpansionJointSpec {
            axis: JointAxis::ConstantU,
            position_m: x,
        }],
        ..JointRules::default()
    };
    let s = build_structure(&cells, &spec, &rules).unwrap();
    assert_eq!(block_sizes(&s), vec![2, 4]);
    assert_eq!(s.joints[0].source, JointSource::Manual);

    // Removing module 4 leaves 0–3 and 5: the row splits.
    cells.remove(4);
    let s = build_structure(&cells, &spec, &JointRules::default()).unwrap();
    assert_eq!(block_sizes(&s), vec![1, 4]);
}

/// Invariants on a larger field with blocks, a thermal rule and a manual joint.
#[test]
fn structure_invariants() {
    let spec = GridSpec {
        footprint_u: 1.134,
        footprint_v: 1.762,
        gap_u: 0.02,
        gap_v: 0.02,
        block: Some(BlockParams {
            max_cols: 8,
            max_rows: 3,
            gap_u_m: 0.1,
            gap_v_m: 0.15,
        }),
        row_pitch_v: None,
        anchor: LayoutAnchor::LowerLeft,
    };
    let mut cells: Vec<Cell> = Vec::new();
    for row in 0..5 {
        for col in 0..19 {
            if (row * 7 + col * 3) % 11 == 0 {
                continue; // a few holes
            }
            cells.push(Cell {
                row,
                col,
                centre: Vec2::new(spec.col_offset(col), spec.row_offset(row)),
            });
        }
    }
    let rules = JointRules {
        max_length_u: Some(5.0),
        max_length_v: Some(4.0),
        manual: vec![ExpansionJointSpec {
            axis: JointAxis::ConstantU,
            position_m: spec.col_offset(12) - 0.5,
        }],
    };
    let s = build_structure(&cells, &spec, &rules).unwrap();
    // Every cell in exactly one block.
    let mut seen = vec![0; cells.len()];
    for b in &s.blocks {
        for &i in &b.cells {
            seen[i] += 1;
            assert_eq!(s.block_of[i], b.id);
        }
        let (w, h) = b.size();
        assert!(
            w <= 5.0 + 1e-9 && h <= 4.0 + 1e-9,
            "block {} is {w:.3} × {h:.3}",
            b.id
        );
        // One lattice block per mounting block.
        let bc = spec.block.unwrap();
        assert!(b
            .cells
            .iter()
            .all(|&i| (cells[i].col / bc.max_cols, cells[i].row / bc.max_rows) == b.lattice_block));
    }
    assert!(seen.iter().all(|&n| n == 1));
    // Links join 4-adjacent cells; rigid ones stay inside a block, the others cross a joint.
    for l in &s.links {
        let (a, b) = (&cells[l.a], &cells[l.b]);
        assert_eq!(a.row.abs_diff(b.row) + a.col.abs_diff(b.col), 1);
        assert_eq!(
            s.block_of[l.a] == s.block_of[l.b],
            !l.crosses_expansion_joint
        );
    }
    // Rigid links connect each block: components of the rigid graph are exactly the blocks.
    for b in &s.blocks {
        let mut reached = vec![b.cells[0]];
        let mut k = 0;
        while k < reached.len() {
            let i = reached[k];
            for l in s.rigid_links() {
                let other = if l.a == i {
                    l.b
                } else if l.b == i {
                    l.a
                } else {
                    continue;
                };
                if !reached.contains(&other) {
                    reached.push(other);
                }
            }
            k += 1;
        }
        assert_eq!(reached.len(), b.cells.len());
    }
    assert!(s.joints.iter().any(|j| j.source == JointSource::Manual));
    assert!(s.joints.iter().any(|j| j.axis == JointAxis::ConstantV));
    let _ = grid_spec; // bridge used in the other tests
}
