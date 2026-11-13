//! L5: several mounting systems on one face, command validation, hardware from the stored
//! parameters.

use pv_layout::model::{hardware, ArrayLayout};
use pv_layout::{layout_face, validate_module_positions, FieldLayout, FieldPlan, Search};
use pv_model::command::keys;
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, BlockParams, Building, BuildingId, CrossRailSpec,
    EdgeSetback, LayoutAnchor, ModuleId, ModuleInstance, ObstacleKind, Orientation, PanelArray,
    Project, ProjectId, RoofEdgeKind, RoofFace, RoofFaceId, RoofKind, RoofObstacle,
    RoofOutlineSpec, SupportGridKind, SupportGridSpec,
};

fn building() -> Building {
    Building {
        id: BuildingId::new(),
        name: "b".into(),
        origin: [0.0, 0.0, 0.0],
        footprint: pv_model::rectangle_footprint([20.0, 12.0]),
        size_m: [20.0, 12.0],
        eave_height_m: 6.0,
        ridge_height_m: 9.0,
        roof_kind: RoofKind::Gable,
        roof: None,
        site: None,
    }
}

/// 20 × 8 m pitched face with a chimney, standing seams every 0.6 m.
fn face(b: &Building) -> RoofFace {
    RoofFace {
        id: RoofFaceId::new(),
        building_id: b.id,
        name: "south".into(),
        origin: [0.0, 0.0, 6.0],
        tilt_deg: 25.0,
        azimuth_deg: 180.0,
        width_m: 20.0,
        height_m: 8.0,
        outline: Some(RoofOutlineSpec {
            vertices_uv: vec![[0.0, 0.0], [20.0, 0.0], [20.0, 8.0], [0.0, 8.0]],
            edge_kinds: vec![
                RoofEdgeKind::Eave,
                RoofEdgeKind::Verge,
                RoofEdgeKind::Ridge,
                RoofEdgeKind::Verge,
            ],
            obstacles: vec![RoofObstacle {
                name: "chimney".into(),
                kind: ObstacleKind::Chimney,
                footprint_uv: vec![[9.5, 5.5], [10.3, 5.5], [10.3, 6.3], [9.5, 6.3]],
                height_m: 1.0,
                clearance_m: None,
            }],
        }),
        support_grid: Some(SupportGridSpec {
            kind: SupportGridKind::StandingSeam,
            origin_m: 0.15,
            spacing_m: 0.6,
        }),
    }
}

/// K2-like SingleRail field (landscape, 2 × 2 blocks) on the left half.
fn singlerail(f: &RoofFace) -> PanelArray {
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: f.id,
        module_length_m: 1.8,
        module_width_m: 1.134,
        orientation: Orientation::Landscape,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        module: None,
        mounting: None,
        layout: ArrayLayoutParams {
            boundary_uv: vec![[0.0, 0.0], [11.0, 0.0], [11.0, 8.0], [0.0, 8.0]],
            module_gap_u_m: 0.01,
            module_gap_v_m: 0.02,
            block: Some(BlockParams {
                max_cols: 2,
                max_rows: 2,
                gap_u_m: 0.08,
                gap_v_m: 0.13,
            }),
            edge_setback_m: 0.3,
            edge_setbacks: vec![EdgeSetback {
                kind: RoofEdgeKind::Ridge,
                distance_m: 0.5,
            }],
            obstacle_clearance_m: 0.3,
            min_distance_to_other_arrays_m: 0.5,
            rails: Some(CrossRailSpec {
                layers: pv_model::RailLayers::Two,
                per_column: 2,
                top_inset_m: 0.4,
                top_overhang_m: 0.05,
                bottom_spacing_m: 1.0,
                min_support_overhang_m: 0.06,
                spacing_step_m: 0.01,
                bottom_inset_m: 0.275,
                fixation_edge_m: 0.03,
                max_cantilever_m: Some(0.485),
            }),
            ..ArrayLayoutParams::default()
        },
    }
}

/// A second system on the same face: portrait modules, no blocks, whole face.
fn portrait(f: &RoofFace) -> PanelArray {
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: f.id,
        module_length_m: 1.762,
        module_width_m: 1.134,
        orientation: Orientation::Portrait,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        module: None,
        mounting: None,
        layout: ArrayLayoutParams {
            module_gap_u_m: 0.02,
            module_gap_v_m: 0.02,
            edge_setback_m: 0.3,
            obstacle_clearance_m: 0.3,
            min_distance_to_other_arrays_m: 0.3,
            anchor: LayoutAnchor::LowerRight,
            ..ArrayLayoutParams::default()
        },
    }
}

fn project(b: Building, f: RoofFace, arrays: Vec<PanelArray>, layouts: &[ArrayLayout]) -> Project {
    let mut modules: Vec<ModuleInstance> = Vec::new();
    for (a, l) in arrays.iter().zip(layouts) {
        let first = modules.len() as u32;
        modules.extend(l.module_instances(a.id, first));
    }
    let mut arrays = arrays;
    for (a, l) in arrays.iter_mut().zip(layouts) {
        a.grid = l.grid;
    }
    Project {
        id: ProjectId::new(),
        name: "mix".into(),
        buildings: vec![b],
        roof_faces: vec![f],
        arrays,
        modules,
        pricing: pv_model::Pricing::default(),
        components: Vec::new(),
        wiring: pv_model::Wiring::default(),
    }
}

fn singles(layouts: Vec<FieldLayout>) -> Vec<ArrayLayout> {
    layouts
        .into_iter()
        .map(|l| match l {
            FieldLayout::Single(a, _) => a,
            FieldLayout::EastWest { .. } => panic!("single expected"),
        })
        .collect()
}

fn mixed() -> Project {
    let b = building();
    let f = face(&b);
    let (sr, pt) = (singlerail(&f), portrait(&f));
    let layouts = singles(
        layout_face(
            &f,
            &[FieldPlan::Single(&sr), FieldPlan::Single(&pt)],
            Search::default(),
        )
        .unwrap(),
    );
    project(b, f, vec![sr, pt], &layouts)
}

#[test]
fn two_systems_share_a_face_without_conflict() {
    let p = mixed();
    let (a0, a1) = (p.arrays[0].id, p.arrays[1].id);
    let n0 = p.modules.iter().filter(|m| m.array_id == a0).count();
    let n1 = p.modules.iter().filter(|m| m.array_id == a1).count();
    assert!(n0 >= 16 && n1 >= 6, "both fields are filled: {n0} + {n1}");
    // The whole result is valid under the command rules (every module checked).
    let all: Vec<ModuleId> = p.modules.iter().map(|m| m.id).collect();
    validate_module_positions(&p, &all).unwrap();
    assert!(pv_layout::face_conflicts(&p, p.roof_faces[0].id).is_empty());
    // The second field keeps 0.5 m (the larger of the two rules) from the first one.
    let fp = |m: &ModuleInstance| {
        pv_layout::validate::footprint(
            p.arrays.iter().find(|a| a.id == m.array_id).unwrap(),
            m.u,
            m.v,
        )
    };
    let mut min_d = f64::INFINITY;
    for m in p.modules.iter().filter(|m| m.array_id == a0) {
        for o in p.modules.iter().filter(|m| m.array_id == a1) {
            let (r, s) = (fp(m), fp(o));
            min_d = min_d.min(
                s.edges()
                    .iter()
                    .map(|e| r.distance_to_segment(e))
                    .fold(f64::INFINITY, f64::min),
            );
        }
    }
    assert!(min_d >= 0.5 - 1e-9, "distance between fields {min_d}");
}

fn moved(p: &Project, id: u32, du: f64, dv: f64) -> Project {
    let mut q = p.clone();
    let m = q.modules.iter_mut().find(|m| m.id == ModuleId(id)).unwrap();
    m.u += du;
    m.v += dv;
    q
}

#[test]
fn command_validation_rejects_each_rule() {
    let p = mixed();
    let a0 = p.arrays[0].id;
    let a1 = p.arrays[1].id;
    let first0 = p.modules.iter().find(|m| m.array_id == a0).unwrap().clone();
    // Moving a module onto its neighbour.
    let q = moved(&p, first0.id.0, 0.5, 0.0);
    assert_eq!(
        validate_module_positions(&q, &[first0.id]).unwrap_err().key,
        keys::LAYOUT_MODULE_OVERLAP
    );
    // Into the eave setback (0.3 m): the bottom row sits at ≥ 0.3 m, push it 0.25 m down.
    let low = p
        .modules
        .iter()
        .filter(|m| m.array_id == a0)
        .min_by(|a, b| a.v.total_cmp(&b.v))
        .unwrap();
    let q = moved(&p, low.id.0, 0.0, -(low.v - 0.567 - 0.05));
    assert_eq!(
        validate_module_positions(&q, &[low.id]).unwrap_err().key,
        keys::LAYOUT_EDGE_SETBACK
    );
    // Onto the chimney.
    let top = p
        .modules
        .iter()
        .filter(|m| m.array_id == a1)
        .max_by(|a, b| a.v.total_cmp(&b.v))
        .unwrap()
        .clone();
    let mut q = p.clone();
    {
        let m = q.modules.iter_mut().find(|m| m.id == top.id).unwrap();
        m.u = 9.9;
        m.v = 5.9;
    }
    let err = validate_module_positions(&q, &[top.id]).unwrap_err();
    assert!(
        [keys::LAYOUT_OBSTACLE_OVERLAP, keys::LAYOUT_MODULE_OVERLAP].contains(&err.key.as_str()),
        "{err:?}"
    );
    // Next to the other field, closer than 0.5 m.
    let right0 = p
        .modules
        .iter()
        .filter(|m| m.array_id == a0)
        .max_by(|a, b| a.u.total_cmp(&b.u))
        .unwrap();
    let left1 = p
        .modules
        .iter()
        .filter(|m| m.array_id == a1)
        .min_by(|a, b| a.u.total_cmp(&b.u))
        .unwrap();
    let gap = (left1.u - 0.567) - (right0.u + 0.9);
    if gap > 0.6 {
        let q = moved(&p, right0.id.0, gap - 0.45, 0.0);
        let err = validate_module_positions(&q, &[right0.id]).unwrap_err();
        assert!(
            [keys::LAYOUT_ARRAY_DISTANCE, keys::LAYOUT_MODULE_OVERLAP].contains(&err.key.as_str()),
            "{err:?}"
        );
    }
    // A harmless move inside the gap is accepted.
    let q = moved(&p, first0.id.0, 0.004, 0.0);
    validate_module_positions(&q, &[first0.id]).unwrap();
}

#[test]
fn hardware_from_stored_parameters() {
    let p = mixed();
    let (f, sr, pt) = (&p.roof_faces[0], &p.arrays[0], &p.arrays[1]);
    let hw = hardware(f, sr, &p.modules, 1)
        .unwrap()
        .expect("SingleRail has rail rules");
    let fx = hw.fixations.as_ref().expect("face has standing seams");
    assert!(!hw.rails.top.is_empty() && !hw.rails.bottom.is_empty());
    // Every fixation sits on a seam (u = 0.15 + k·0.6), at ≥ 0.03 m from its rail ends.
    for (rail, pos) in fx.bottom.iter().zip(&fx.positions) {
        assert!(pos.len() >= 2);
        for &u in pos {
            let k = (u - 0.15) / 0.6;
            assert!((k - k.round()).abs() < 1e-9, "fixation off-seam at {u}");
            assert!(u >= rail.start.u + 0.03 - 1e-9 && u <= rail.end.u - 0.03 + 1e-9);
        }
    }
    // No rail rules → no hardware for the other system.
    assert!(hardware(f, pt, &p.modules, 1).unwrap().is_none());
}

#[test]
fn east_west_and_tilted_rows_on_one_flat_roof() {
    let b = building();
    let flat = RoofFace {
        id: RoofFaceId::new(),
        building_id: b.id,
        name: "flat".into(),
        origin: [0.0, 0.0, 6.0],
        tilt_deg: 0.0,
        azimuth_deg: 180.0,
        width_m: 24.0,
        height_m: 12.0,
        outline: None,
        support_grid: None,
    };
    let ew_layout = ArrayLayoutParams {
        boundary_uv: vec![[0.0, 0.0], [14.0, 0.0], [14.0, 12.0], [0.0, 12.0]],
        module_gap_u_m: 0.08,
        module_gap_v_m: 0.038,
        edge_setback_m: 0.6,
        row_pitch_m: Some(2.46),
        min_distance_to_other_arrays_m: 1.0,
        ..ArrayLayoutParams::default()
    };
    // East-west halves are landscape (long side along v, short side tilted: D-Dome);
    // the south rows portrait.
    let mk = |az: f64, tilt: f64, layout: ArrayLayoutParams| PanelArray {
        id: ArrayId::new(),
        roof_face_id: flat.id,
        module_length_m: 1.762,
        module_width_m: 1.134,
        orientation: if az == 0.0 {
            Orientation::Portrait
        } else {
            Orientation::Landscape
        },
        tilt_on_roof_deg: tilt,
        azimuth_offset_deg: az,
        grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
        layout,
        module: None,
        mounting: None,
    };
    let mut west = mk(90.0, 8.6, ew_layout.clone());
    let mut east = mk(-90.0, 8.6, ew_layout);
    // The two halves of one east-west system.
    west.layout.paired_with = Some(east.id);
    east.layout.paired_with = Some(west.id);
    let south = mk(
        0.0,
        15.0,
        ArrayLayoutParams {
            module_gap_u_m: 0.02,
            edge_setback_m: 0.6,
            row_pitch_m: Some(2.6),
            min_distance_to_other_arrays_m: 1.0,
            anchor: LayoutAnchor::LowerRight,
            ..ArrayLayoutParams::default()
        },
    );
    let out = layout_face(
        &flat,
        &[FieldPlan::EastWest(&west, &east), FieldPlan::Single(&south)],
        Search::default(),
    )
    .unwrap();
    let (
        FieldLayout::EastWest {
            plus90,
            minus90,
            pairs,
        },
        FieldLayout::Single(s, _),
    ) = (&out[0], &out[1])
    else {
        panic!("plan order kept");
    };
    assert_eq!(plus90.cells.len(), minus90.cells.len());
    assert_eq!(pairs.block_of.len(), plus90.cells.len());
    assert!(!s.cells.is_empty());
    let p = project(
        b,
        flat,
        vec![west, east, south],
        &[plus90.clone(), minus90.clone(), s.clone()],
    );
    let all: Vec<ModuleId> = p.modules.iter().map(|m| m.id).collect();
    validate_module_positions(&p, &all).unwrap();
}
