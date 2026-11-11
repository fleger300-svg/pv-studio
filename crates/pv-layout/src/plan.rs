//! Automatic layout as an explicit command batch (the journal records the batch, not
//! « lay out », so a later version of the engine never changes an existing history).
//!
//! Every array of the face is laid out again, in project order (east-west pairs together),
//! each seeing the fields already placed as keep-out areas: an array never overlaps its
//! neighbours, and changing one array's settings may move the others. The batch upserts
//! each array with its new grid and replaces its modules: one undo reverts it all.

use pv_model::command::keys;
use pv_model::project::MODULES_PER_STRING;
use pv_model::stress::{EDGE_SETBACK_M, MODULE_GAP_M, MODULE_LENGTH_M, MODULE_WIDTH_M};
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, Command, CommandError, CrossRailSpec, ModuleSlot,
    ModuleState, Orientation, PanelArray, Project, RoofFace,
};

use crate::grid::{Cell, Search};
use crate::mixing::{layout_face, FieldLayout, FieldPlan};
use crate::model::ArrayLayout;
use crate::LayoutError;

impl LayoutError {
    /// The error as the user sees it (i18n key and parameters).
    #[must_use]
    pub fn command_error(&self) -> CommandError {
        match self {
            LayoutError::Outline(_)
            | LayoutError::EdgeKindCount { .. }
            | LayoutError::ObstacleOffRoof { .. } => {
                CommandError::new(keys::LAYOUT_INVALID_OUTLINE)
            }
            other => CommandError::new(keys::LAYOUT_INVALID_PARAMETERS)
                .with_text("detail", other.to_string()),
        }
    }
}

/// A new array for a face: the reference module (portrait), 2 cm gaps, 30 cm setbacks,
/// K2 SingleRail rail geometry; no components yet (the shell assigns the library's module and
/// system with `SetArrayComponents`) and no modules (the layout places them).
#[must_use]
pub fn default_array(face: &RoofFace) -> PanelArray {
    PanelArray {
        id: ArrayId::new(),
        roof_face_id: face.id,
        module_length_m: MODULE_LENGTH_M,
        module_width_m: MODULE_WIDTH_M,
        orientation: Orientation::Portrait,
        tilt_on_roof_deg: 0.0,
        azimuth_offset_deg: 0.0,
        grid: ArrayGrid::regular(
            0,
            0,
            EDGE_SETBACK_M,
            EDGE_SETBACK_M,
            MODULE_WIDTH_M + MODULE_GAP_M,
            MODULE_LENGTH_M + MODULE_GAP_M,
        ),
        layout: ArrayLayoutParams {
            module_gap_u_m: MODULE_GAP_M,
            module_gap_v_m: MODULE_GAP_M,
            edge_setback_m: EDGE_SETBACK_M,
            obstacle_clearance_m: EDGE_SETBACK_M,
            min_distance_to_other_arrays_m: 0.5,
            rails: Some(CrossRailSpec::k2_single_rail()),
            ..ArrayLayoutParams::default()
        },
        module: None,
        mounting: None,
    }
}

fn slots(cells: &[Cell]) -> Vec<ModuleSlot> {
    cells
        .iter()
        .enumerate()
        .map(|(k, c)| ModuleSlot {
            row: c.row,
            col: c.col,
            u: c.centre.u,
            v: c.centre.v,
            state: ModuleState::Active,
            string_index: k as u32 / MODULES_PER_STRING,
        })
        .collect()
}

fn place(array: &PanelArray, layout: &ArrayLayout, out: &mut Vec<Command>) {
    out.push(Command::UpsertArray {
        array: Box::new(PanelArray {
            grid: layout.grid,
            ..array.clone()
        }),
        at: None,
    });
    out.push(Command::SetArrayModules {
        array_id: array.id,
        modules: slots(&layout.cells),
    });
}

/// The batch that gives `array` (new or changed) its settings and lays out its whole face.
///
/// # Errors
/// The array's own validation (unknown face…) or the layout engine's (invalid outline,
/// pitch smaller than the module…), as translatable errors.
pub fn plan_layout(
    project: &Project,
    array: &PanelArray,
    search: Search,
) -> Result<Command, CommandError> {
    let mut work = project.clone();
    Command::UpsertArray {
        array: Box::new(array.clone()),
        at: None,
    }
    .apply(&mut work)?;
    // Arrays laid out before their layout parameters were stored (older projects, imports)
    // keep what their grid implies — the batch then stores it explicitly:
    // - the pair pitch of east-west halves is the pitch of their grid;
    // - module gaps are the grid pitch minus the module footprint (unless the user just
    //   entered a gap for this array: an explicit value always wins).
    let stored = project.array_index(array.id).map(|i| &project.arrays[i]);
    for a in &mut work.arrays {
        if a.layout.paired_with.is_some() && a.layout.row_pitch_m.is_none() && a.grid.pitch_u > 0.0
        {
            a.layout.row_pitch_m = Some(a.grid.pitch_u);
        }
        let edited = a.id == array.id;
        let (hu, hv) = a.half_footprint();
        let implied_u = a.grid.pitch_u - 2.0 * hu;
        let implied_v = a.grid.pitch_v - 2.0 * hv;
        let untouched = |now: f64, before: Option<f64>| !edited || before == Some(now);
        if a.layout.module_gap_u_m == 0.0
            && implied_u > 1e-9
            && a.layout.paired_with.is_none()
            && untouched(0.0, stored.map(|s| s.layout.module_gap_u_m))
        {
            a.layout.module_gap_u_m = implied_u;
        }
        if a.layout.module_gap_v_m == 0.0
            && implied_v > 1e-9
            && a.layout.row_pitch_m.is_none()
            && untouched(0.0, stored.map(|s| s.layout.module_gap_v_m))
        {
            a.layout.module_gap_v_m = implied_v;
        }
    }
    let face = work
        .roof_face(array.roof_face_id)
        .ok_or_else(|| CommandError::new(keys::ENTITY_REFERENCE).with_text("kind", "roofFace"))?
        .clone();
    let on_face: Vec<&PanelArray> = work
        .arrays
        .iter()
        .filter(|a| a.roof_face_id == face.id)
        .collect();
    // East-west pairs are laid out together, at the place of their +90° half.
    let mut plans: Vec<FieldPlan<'_>> = Vec::new();
    for a in &on_face {
        let partner = a
            .layout
            .paired_with
            .and_then(|p| on_face.iter().find(|b| b.id == p).copied());
        match partner {
            Some(b) if (a.azimuth_offset_deg - 90.0).abs() < 1e-6 => {
                plans.push(FieldPlan::EastWest(a, b));
            }
            Some(b) if (b.azimuth_offset_deg - 90.0).abs() < 1e-6 => {}
            _ => plans.push(FieldPlan::Single(a)),
        }
    }
    let layouts = layout_face(&face, &plans, search).map_err(|e| e.command_error())?;
    let mut commands = Vec::new();
    for (plan, layout) in plans.iter().zip(&layouts) {
        match (plan, layout) {
            (FieldPlan::Single(a), FieldLayout::Single(l, _)) => place(a, l, &mut commands),
            (
                FieldPlan::EastWest(p, m),
                FieldLayout::EastWest {
                    plus90, minus90, ..
                },
            ) => {
                place(p, plus90, &mut commands);
                place(m, minus90, &mut commands);
            }
            _ => unreachable!("layout_face answers each plan with its own kind"),
        }
    }
    Ok(Command::Batch { commands })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pv_model::StressPreset;

    fn bits(p: &Project) -> String {
        serde_json::to_string(p).expect("json")
    }

    #[test]
    fn relayout_after_a_setting_change_is_one_exact_undo() {
        let mut p = StressPreset::Residential1k.build();
        let original = bits(&p);
        let mut a = p.arrays[0].clone();
        a.layout.edge_setback_m = 0.8;
        let before = p.modules.iter().filter(|m| m.array_id == a.id).count();
        let cmd = plan_layout(&p, &a, Search::default()).expect("plan");
        let inverse = cmd.inverse(&p);
        cmd.apply(&mut p).expect("apply");
        p.validate().expect("invariants");
        let after = p.modules.iter().filter(|m| m.array_id == a.id).count();
        assert!(
            after > 0 && after < before,
            "{before} → {after}: wider setbacks, fewer modules"
        );
        let ids: Vec<_> = p
            .modules
            .iter()
            .filter(|m| m.array_id == a.id)
            .map(|m| m.id)
            .collect();
        crate::validate_module_positions(&p, &ids).expect("the layout passes the layout rules");
        inverse.apply(&mut p).expect("undo");
        assert_eq!(bits(&p), original);
    }

    #[test]
    fn a_second_array_on_a_face_keeps_its_distance() {
        let mut p = StressPreset::Residential1k.build();
        let face = p.roof_faces[0].clone();
        // Make room: the first array keeps the left half of the face.
        let mut first = p.arrays[0].clone();
        first.layout.boundary_uv = vec![
            [0.0, 0.0],
            [face.width_m / 2.0, 0.0],
            [face.width_m / 2.0, face.height_m],
            [0.0, face.height_m],
        ];
        plan_layout(&p, &first, Search::default())
            .unwrap()
            .apply(&mut p)
            .unwrap();
        let second = default_array(&face);
        let cmd = plan_layout(&p, &second, Search::default()).expect("plan");
        cmd.apply(&mut p).expect("apply");
        p.validate().expect("invariants");
        let new: Vec<_> = p
            .modules
            .iter()
            .filter(|m| m.array_id == second.id)
            .map(|m| m.id)
            .collect();
        assert!(!new.is_empty(), "the second array got modules");
        let all: Vec<_> = p
            .modules
            .iter()
            .filter(|m| m.array_id == second.id || m.array_id == first.id)
            .map(|m| m.id)
            .collect();
        crate::validate_module_positions(&p, &all).expect("no overlap, distance kept");
    }

    #[test]
    fn layout_errors_are_translatable() {
        let p = StressPreset::Residential1k.build();
        let mut a = p.arrays[0].clone();
        a.layout.row_pitch_m = Some(0.5); // smaller than the module
        let e = plan_layout(&p, &a, Search::default()).expect_err("refused");
        assert_eq!(e.key, keys::LAYOUT_INVALID_PARAMETERS);
    }

    #[test]
    fn east_west_pairs_are_laid_out_together() {
        let mut p = StressPreset::Industrial10k.build();
        let original = bits(&p);
        let mut a = p.arrays[0].clone();
        a.layout.edge_setback_m += 0.5;
        let cmd = plan_layout(&p, &a, Search::default()).expect("plan");
        let Command::Batch { commands } = &cmd else {
            panic!()
        };
        let counts: Vec<usize> = commands
            .iter()
            .filter_map(|c| match c {
                Command::SetArrayModules { modules, .. } => Some(modules.len()),
                _ => None,
            })
            .collect();
        assert_eq!(counts.len(), 2, "both halves");
        assert_eq!(counts[0], counts[1], "one +90° module per −90° module");
        let inverse = cmd.inverse(&p);
        cmd.apply(&mut p).expect("apply");
        p.validate().expect("invariants");
        assert!(
            p.arrays.iter().all(|x| x.layout.row_pitch_m.is_some()),
            "pair pitch stored"
        );
        let ids: Vec<_> = p.modules.iter().map(|m| m.id).collect();
        crate::validate_module_positions(&p, &ids).expect("layout rules");
        inverse.apply(&mut p).expect("undo");
        assert_eq!(bits(&p), original);
    }

    #[test]
    fn gaps_implied_by_an_older_grid_are_kept() {
        let mut p = StressPreset::Residential1k.build();
        let a = p.arrays[0].clone();
        assert_eq!(
            a.layout.module_gap_u_m, 0.0,
            "preset built without stored gaps"
        );
        let mut changed = a.clone();
        changed.layout.edge_setback_m = 0.5;
        plan_layout(&p, &changed, Search::default())
            .unwrap()
            .apply(&mut p)
            .unwrap();
        let now = &p.arrays[0];
        assert!(
            (now.layout.module_gap_u_m - MODULE_GAP_M).abs() < 1e-9,
            "{}",
            now.layout.module_gap_u_m
        );
        assert!((now.layout.module_gap_v_m - MODULE_GAP_M).abs() < 1e-9);
        // An explicit 0 entered by the user wins.
        let mut zero = now.clone();
        zero.layout.module_gap_u_m = 0.0;
        plan_layout(&p, &zero, Search::default())
            .unwrap()
            .apply(&mut p)
            .unwrap();
        assert_eq!(p.arrays[0].layout.module_gap_u_m, 0.0);
    }
}
