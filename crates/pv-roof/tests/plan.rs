//! E2.3: the generated roof as an explicit command batch on a real project.

mod common;

use common::*;
use pv_model::{
    BuildingId, Command, ObstacleKind, Project, RoofFaceId, RoofObstacle, RoofOutlineSpec,
    StressPreset,
};
use pv_roof::plan::plan_roof_with;
use pv_roof::presets;
use uuid::Uuid;

fn ids() -> impl FnMut() -> RoofFaceId {
    let mut n = 1000u128;
    move || {
        n += 1;
        RoofFaceId(Uuid::from_u128(n))
    }
}

fn bits(p: &Project) -> String {
    serde_json::to_string(p).unwrap()
}

/// Applies a plan like the store would, checks invariants, undo and redo.
fn apply_exactly(p: &mut Project, cmd: &Command) {
    let before = bits(p);
    let inverse = cmd.inverse(p);
    cmd.apply(p).expect("plan applies");
    p.validate().expect("invariants");
    let after = bits(p);
    inverse.apply(p).expect("undo");
    assert_eq!(bits(p), before, "undo is exact");
    cmd.apply(p).expect("redo");
    assert_eq!(bits(p), after, "redo is exact");
}

fn faces_of(p: &Project, b: BuildingId) -> Vec<RoofFaceId> {
    p.roof_faces
        .iter()
        .filter(|f| f.building_id == b)
        .map(|f| f.id)
        .collect()
}

#[test]
fn same_roof_keeps_every_face_and_array() {
    let mut p = StressPreset::Residential1k.build();
    let mut b = p.buildings[0].clone();
    let before = faces_of(&p, b.id);
    b.roof = Some(presets::gable(&b.footprint, 30.0, 0.0));
    let cmd = plan_roof_with(&p, &b, ids()).unwrap();
    let Command::Batch { commands } = &cmd else {
        panic!()
    };
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::RemoveRoofFace { .. })),
        "nothing removed"
    );
    let arrays = p.arrays.len();
    apply_exactly(&mut p, &cmd);
    assert_eq!(faces_of(&p, b.id), before, "ids kept");
    assert_eq!(p.arrays.len(), arrays, "arrays still attached");
    assert_eq!(p.roof_faces[0].name, "South");
}

#[test]
fn gable_to_hipped_keeps_the_long_faces_and_adds_the_ends() {
    let mut p = StressPreset::Residential1k.build();
    let mut b = p.buildings[1].clone();
    let before = faces_of(&p, b.id);
    b.roof = Some(presets::hipped(&b.footprint, 30.0, 45.0, 0.3));
    let cmd = plan_roof_with(&p, &b, ids()).unwrap();
    apply_exactly(&mut p, &cmd);
    let after = faces_of(&p, b.id);
    assert_eq!(after.len(), 4);
    assert!(
        before.iter().all(|id| after.contains(id)),
        "trapezoids took over the rectangles"
    );
    let b2 = p.buildings.iter().find(|x| x.id == b.id).unwrap();
    assert_eq!(b2.roof_kind, pv_model::RoofKind::Hipped);
    assert!(
        p.arrays
            .iter()
            .filter(|a| before.contains(&a.roof_face_id))
            .count()
            == 2
    );
}

#[test]
fn a_face_that_disappears_is_removed_with_its_array() {
    let mut p = StressPreset::Residential1k.build();
    let mut b = p.buildings[2].clone();
    let north = faces_of(&p, b.id)[1];
    // Monopitch draining south: one face covering everything; it takes over the south face
    // (same orientation, IoU 50 %), the north face disappears.
    b.roof = Some(presets::monopitch(&b.footprint, 0, 15.0, 0.0));
    let cmd = plan_roof_with(&p, &b, ids()).unwrap();
    let Command::Batch { commands } = &cmd else {
        panic!()
    };
    assert!(commands.contains(&Command::RemoveRoofFace { id: north }));
    let arrays = p.arrays.len();
    apply_exactly(&mut p, &cmd);
    assert_eq!(p.arrays.len(), arrays - 1);
}

#[test]
fn a_new_building_gets_new_faces() {
    let mut p = StressPreset::Residential1k.build();
    let fp = vec![
        [0.0, 0.0],
        [12.0, 0.0],
        [14.0, 5.0],
        [6.0, 9.0],
        [-2.0, 5.0],
    ];
    let mut b = building(fp.clone(), presets::hipped(&fp, 35.0, 35.0, 0.4));
    b.id = BuildingId(Uuid::from_u128(77));
    let cmd = plan_roof_with(&p, &b, ids()).unwrap();
    apply_exactly(&mut p, &cmd);
    assert_eq!(faces_of(&p, b.id).len(), 5);
    // Planning again on the result changes nothing.
    let b_now = p.buildings.iter().find(|x| x.id == b.id).unwrap().clone();
    let again = plan_roof_with(&p, &b_now, ids()).unwrap();
    assert_eq!(
        again,
        Command::Batch {
            commands: vec![Command::UpsertBuilding {
                building: Box::new(b_now),
                at: None
            }]
        }
    );
}

#[test]
fn obstacles_stay_where_they_are_in_the_world() {
    let mut p = StressPreset::Residential1k.build();
    let mut b = p.buildings[0].clone();
    let fid = faces_of(&p, b.id)[0];
    let fi = p.roof_faces.iter().position(|f| f.id == fid).unwrap();
    let (w, h) = (p.roof_faces[fi].width_m, p.roof_faces[fi].height_m);
    p.roof_faces[fi].outline = Some(RoofOutlineSpec {
        vertices_uv: vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
        edge_kinds: vec![
            pv_model::RoofEdgeKind::Eave,
            pv_model::RoofEdgeKind::Verge,
            pv_model::RoofEdgeKind::Ridge,
            pv_model::RoofEdgeKind::Verge,
        ],
        obstacles: vec![RoofObstacle {
            name: "Chimney".into(),
            kind: ObstacleKind::Chimney,
            footprint_uv: vec![[10.0, 3.0], [11.0, 3.0], [11.0, 4.0], [10.0, 4.0]],
            height_m: 1.0,
            clearance_m: None,
        }],
    });
    let world_before = p.roof_faces[fi].frame().to_world(10.0, 3.0, 0.0);
    // An overhang moves the face origin: the chimney must not move.
    b.roof = Some(presets::gable(&b.footprint, 30.0, 0.6));
    let cmd = plan_roof_with(&p, &b, ids()).unwrap();
    apply_exactly(&mut p, &cmd);
    let f = p.roof_faces.iter().find(|f| f.id == fid).unwrap();
    let ob = &f.outline.as_ref().unwrap().obstacles[0];
    let [u, v] = ob.footprint_uv[0];
    let world_after = f.frame().to_world(u, v, 0.0);
    for i in 0..3 {
        assert!((world_after[i] - world_before[i]).abs() < 1e-9);
    }
    assert!((u - 10.6).abs() < 1e-9, "shifted by the overhang in u: {u}");
}

#[test]
fn invalid_specs_are_refused() {
    let p = StressPreset::Residential1k.build();
    let mut b = p.buildings[0].clone();
    assert_eq!(
        plan_roof_with(&p, &b, ids()),
        Err(pv_roof::RoofError::NoSpec)
    );
    let mut spec = presets::gable(&b.footprint, 30.0, 0.0);
    spec.edges.pop();
    b.roof = Some(spec);
    assert_eq!(
        plan_roof_with(&p, &b, ids()),
        Err(pv_roof::RoofError::Invalid("roofEdges"))
    );
}
