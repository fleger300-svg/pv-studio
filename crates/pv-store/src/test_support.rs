//! Deterministic fixture project for tests (geometry is fixed; only UUIDs vary).

use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, Building, BuildingId, ModuleId, ModuleInstance,
    ModuleState, Orientation, PanelArray, Project, ProjectId, RoofFace, RoofFaceId, RoofKind,
};

/// Two roof faces: a 30° south face with a coplanar portrait array (2 × 2) and a flat roof
/// with a 10° west-facing landscape rack (1 × 2). Module 2 is removed.
pub fn fixture_project() -> Project {
    let house = Building {
        id: BuildingId::new(),
        name: "House".into(),
        origin: [5.0, 0.0, 0.0],
        size_m: [10.0, 8.0],
        eave_height_m: 5.0,
        ridge_height_m: 7.0,
        roof_kind: RoofKind::Gable,
    };
    let shed = Building {
        id: BuildingId::new(),
        name: "Shed".into(),
        origin: [20.0, 0.0, 0.0],
        size_m: [8.0, 6.0],
        eave_height_m: 3.0,
        ridge_height_m: 3.0,
        roof_kind: RoofKind::Flat,
    };
    let south = RoofFace {
        id: RoofFaceId::new(),
        building_id: house.id,
        name: "South".into(),
        origin: [0.0, -4.0, 5.0],
        tilt_deg: 30.0,
        azimuth_deg: 180.0,
        width_m: 10.0,
        height_m: 4.6,
        outline: None,
    };
    let flat = RoofFace {
        id: RoofFaceId::new(),
        building_id: shed.id,
        name: "Flat".into(),
        origin: [16.0, -3.0, 3.0],
        tilt_deg: 0.0,
        azimuth_deg: 180.0,
        width_m: 8.0,
        height_m: 6.0,
        outline: None,
    };
    let coplanar = PanelArray {
        id: ArrayId::new(),
        roof_face_id: south.id,
        module_length_m: 1.762,
        module_width_m: 1.134,
        orientation: Orientation::Portrait,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid {
            rows: 2,
            cols: 2,
            origin_u: 1.0,
            origin_v: 1.25,
            pitch_u: 1.154,
            pitch_v: 1.782,
            block_cols: 0,
            block_rows: 0,
            block_extra_u: 0.0,
            block_extra_v: 0.0,
        },
        layout: ArrayLayoutParams::default(),
    };
    let rack = PanelArray {
        id: ArrayId::new(),
        roof_face_id: flat.id,
        module_length_m: 1.762,
        module_width_m: 1.134,
        orientation: Orientation::Landscape,
        tilt_on_roof_deg: 10.0,
        azimuth_offset_deg: 90.0,
        grid: ArrayGrid {
            rows: 2,
            cols: 1,
            origin_u: 1.0,
            origin_v: 1.5,
            pitch_u: 1.2,
            pitch_v: 1.782,
            block_cols: 0,
            block_rows: 0,
            block_extra_u: 0.0,
            block_extra_v: 0.0,
        },
        layout: ArrayLayoutParams::default(),
    };
    let mut modules = Vec::new();
    for (array, rows, cols) in [(&coplanar, 2u32, 2u32), (&rack, 2, 1)] {
        for row in 0..rows {
            for col in 0..cols {
                let id = modules.len() as u32;
                modules.push(ModuleInstance {
                    id: ModuleId(id),
                    array_id: array.id,
                    row,
                    col,
                    u: array.grid.origin_u + f64::from(col) * array.grid.pitch_u,
                    v: array.grid.origin_v + f64::from(row) * array.grid.pitch_v,
                    state: if id == 2 {
                        ModuleState::Removed
                    } else {
                        ModuleState::Active
                    },
                    string_index: id / 3,
                });
            }
        }
    }
    Project {
        id: ProjectId::new(),
        name: "Fixture".into(),
        buildings: vec![house, shed],
        roof_faces: vec![south, flat],
        arrays: vec![coplanar, rack],
        modules,
    }
}
