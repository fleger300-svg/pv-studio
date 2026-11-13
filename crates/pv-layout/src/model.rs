//! Bridge with `pv-model`: lays out a `PanelArray` on its `RoofFace` and produces the
//! `ArrayGrid` and `ModuleInstance`s the rest of the application uses.

use pv_geo::{Polygon, Vec2};
use pv_model::{ArrayGrid, ArrayId, ModuleId, ModuleInstance, ModuleState, PanelArray, RoofFace};
use serde::Serialize;

use crate::blocks::{build_structure, Structure};
use crate::grid::{
    fill, fill_east_west, place_fixed, Cell, EastWestSpec, GridSpec, Placement, Region, Search,
};
use crate::joints::JointRules;
use crate::rails::{cross_rails, fixations, CrossRailParams, Fixations, RailLayout};
use crate::roof::RoofOutline;
use crate::setbacks::{BuildableArea, SetbackRules};
use crate::supports::{SupportKind, SupportLines};
use crate::LayoutError;

/// Setback rules of an array.
#[must_use]
pub fn setback_rules(array: &PanelArray) -> SetbackRules {
    let p = &array.layout;
    let mut rules = SetbackRules::uniform(p.edge_setback_m, p.obstacle_clearance_m);
    for s in &p.edge_setbacks {
        rules = rules.with_edge(s.kind, s.distance_m);
    }
    rules
}

/// Buildable area of an array on its face.
pub fn buildable_area(face: &RoofFace, array: &PanelArray) -> Result<BuildableArea, LayoutError> {
    Ok(BuildableArea::new(
        RoofOutline::from_face(face)?,
        &setback_rules(array),
    ))
}

/// Field outline of an array, if it has one.
pub fn field_boundary(array: &PanelArray) -> Result<Option<Polygon>, LayoutError> {
    let b = &array.layout.boundary_uv;
    if b.is_empty() {
        return Ok(None);
    }
    Ok(Some(Polygon::new(
        b.iter().map(|&[u, v]| Vec2::new(u, v)).collect(),
    )?))
}

/// Grid geometry of an array: footprint from `PanelArray::half_footprint` (tilt and azimuth
/// offset included, exactly as the 3D view draws it), gaps and blocks from its parameters.
#[must_use]
pub fn grid_spec(array: &PanelArray) -> GridSpec {
    let (hu, hv) = array.half_footprint();
    let p = &array.layout;
    GridSpec {
        footprint_u: 2.0 * hu,
        footprint_v: 2.0 * hv,
        gap_u: p.module_gap_u_m,
        gap_v: p.module_gap_v_m,
        block: p.block,
        row_pitch_v: p.row_pitch_m,
        anchor: p.anchor,
    }
}

/// Laid-out array: its grid and module centres.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrayLayout {
    pub grid: ArrayGrid,
    pub cells: Vec<Cell>,
}

impl ArrayLayout {
    pub(crate) fn from_placement(p: &Placement) -> Self {
        let (bc, br, eu, ev) = p.spec.block_terms();
        let (su, sv) = match p.spec.anchor {
            pv_model::LayoutAnchor::LowerRight => (-1.0, 1.0),
            pv_model::LayoutAnchor::UpperLeft => (1.0, -1.0),
            pv_model::LayoutAnchor::UpperRight => (-1.0, -1.0),
            _ => (1.0, 1.0),
        };
        // `ArrayGrid` grows towards +u / +v; mirrored lattices are stored with negative pitch.
        let grid = ArrayGrid {
            rows: p.rows,
            cols: p.cols,
            origin_u: p.origin.u,
            origin_v: p.origin.v,
            pitch_u: su * p.spec.pitch_u(),
            pitch_v: sv * p.spec.pitch_v(),
            block_cols: bc,
            block_rows: br,
            block_extra_u: su * eu,
            block_extra_v: sv * ev,
        };
        Self {
            grid,
            cells: p.cells.clone(),
        }
    }

    /// Module instances with consecutive ids starting at `first_id`.
    #[must_use]
    pub fn module_instances(&self, array_id: ArrayId, first_id: u32) -> Vec<ModuleInstance> {
        self.cells
            .iter()
            .enumerate()
            .map(|(i, c)| ModuleInstance {
                id: ModuleId(first_id + i as u32),
                array_id,
                row: c.row,
                col: c.col,
                u: c.centre.u,
                v: c.centre.v,
                state: ModuleState::Active,
                string_index: 0,
            })
            .collect()
    }
}

/// The pair pitch of an east-west system must hold a whole pair.
pub(crate) fn ew_pitch_check(spec: &EastWestSpec) -> Result<(), LayoutError> {
    let pair = 2.0 * spec.footprint_u + spec.peak_gap;
    if spec.pair_pitch_u < pair - 1e-9 {
        return Err(LayoutError::PitchTooSmall {
            axis: "pair",
            pitch_m: spec.pair_pitch_u,
            footprint_m: pair,
        });
    }
    if spec.gap_v < 0.0 {
        return Err(LayoutError::PitchTooSmall {
            axis: "v",
            pitch_m: spec.footprint_v + spec.gap_v,
            footprint_m: spec.footprint_v,
        });
    }
    Ok(())
}

/// Fills the array's field (or the whole face) with the best offset.
pub fn layout_array(
    face: &RoofFace,
    array: &PanelArray,
    search: Search,
) -> Result<ArrayLayout, LayoutError> {
    if array.roof_face_id != face.id {
        return Err(LayoutError::WrongFace);
    }
    let area = buildable_area(face, array)?;
    let boundary = field_boundary(array)?;
    let region = Region {
        area: &area,
        boundary: boundary.as_ref(),
    };
    let spec = grid_spec(array);
    spec.validate()?;
    Ok(ArrayLayout::from_placement(&fill(region, spec, search)))
}

/// Places the array's grid as stored (`array.grid`) and keeps the cells that fit: used to
/// re-validate a layout after the face or the setbacks changed.
pub fn relayout_fixed(face: &RoofFace, array: &PanelArray) -> Result<ArrayLayout, LayoutError> {
    let area = buildable_area(face, array)?;
    let boundary = field_boundary(array)?;
    let region = Region {
        area: &area,
        boundary: boundary.as_ref(),
    };
    let g = &array.grid;
    let p = place_fixed(
        region,
        grid_spec(array),
        Vec2::new(g.origin_u, g.origin_v),
        g.rows,
        g.cols,
    );
    Ok(ArrayLayout::from_placement(&p))
}

/// East-west system stored as two `pv-model` arrays (`+90°` and `−90°` azimuth offset, the
/// `+90°` modules on the −u side of each pair). Both arrays must share the face, module,
/// tilt and layout parameters; `row_pitch_m` is the pair pitch along `u`.
pub fn layout_east_west(
    face: &RoofFace,
    plus90: &PanelArray,
    minus90: &PanelArray,
    search: Search,
) -> Result<(ArrayLayout, ArrayLayout), LayoutError> {
    if plus90.roof_face_id != face.id || minus90.roof_face_id != face.id {
        return Err(LayoutError::WrongFace);
    }
    if (plus90.azimuth_offset_deg - 90.0).abs() > 1e-6
        || (minus90.azimuth_offset_deg + 90.0).abs() > 1e-6
    {
        return Err(LayoutError::NotEastWest);
    }
    let pair_pitch = plus90
        .layout
        .row_pitch_m
        .ok_or(LayoutError::MissingPairPitch)?;
    let area = buildable_area(face, plus90)?;
    let boundary = field_boundary(plus90)?;
    let (hu, hv) = plus90.half_footprint();
    let p = &plus90.layout;
    let spec = EastWestSpec {
        footprint_u: 2.0 * hu,
        footprint_v: 2.0 * hv,
        peak_gap: p.module_gap_u_m,
        pair_pitch_u: pair_pitch,
        gap_v: p.module_gap_v_m,
        block: p.block,
        anchor: p.anchor,
    };
    ew_pitch_check(&spec)?;
    let ew = fill_east_west(
        Region {
            area: &area,
            boundary: boundary.as_ref(),
        },
        spec,
        search,
    );
    let half = (spec.footprint_u + spec.peak_gap) / 2.0;
    let side = |cells: &[Cell], shift: f64| {
        let mut l = ArrayLayout::from_placement(&ew.pairs);
        l.grid.origin_u += shift;
        l.cells = cells.to_vec();
        l
    };
    Ok((side(&ew.minus_u, -half), side(&ew.plus_u, half)))
}

/// Mounting blocks of an array from its stored modules (active ones only, removed modules
/// leave a hole). Returns the structure and, for each of its cells, the module id.
pub fn structure(
    array: &PanelArray,
    modules: &[ModuleInstance],
) -> Result<(Structure, Vec<ModuleId>), LayoutError> {
    let active: Vec<&ModuleInstance> = modules
        .iter()
        .filter(|m| m.array_id == array.id && m.state == ModuleState::Active)
        .collect();
    let cells: Vec<Cell> = active
        .iter()
        .map(|m| Cell {
            row: m.row,
            col: m.col,
            centre: Vec2::new(m.u, m.v),
        })
        .collect();
    let s = build_structure(
        &cells,
        &grid_spec(array),
        &JointRules::from_params(&array.layout),
    )?;
    Ok((s, active.iter().map(|m| m.id).collect()))
}

/// Support lines of a face (seams / rafters), if the face has them.
#[must_use]
pub fn support_lines(face: &RoofFace) -> Option<SupportLines> {
    face.support_grid.map(|g| SupportLines {
        kind: match g.kind {
            pv_model::SupportGridKind::StandingSeam => SupportKind::StandingSeam,
            pv_model::SupportGridKind::Rafters => SupportKind::Rafters,
        },
        origin_m: g.origin_m,
        spacing_m: g.spacing_m,
    })
}

/// Rail rules of an array's mounting system, if it is a cross-rail system.
#[must_use]
pub fn rail_params(array: &PanelArray) -> Option<(CrossRailParams, f64, Option<f64>)> {
    array.layout.rails.map(|r| {
        (
            CrossRailParams {
                layers: r.layers,
                per_column: r.per_column,
                top_inset_m: r.top_inset_m,
                top_overhang_m: r.top_overhang_m,
                bottom_spacing_m: r.bottom_spacing_m,
                min_support_overhang_m: r.min_support_overhang_m,
                spacing_step_m: r.spacing_step_m,
                bottom_inset_m: r.bottom_inset_m,
            },
            r.fixation_edge_m,
            r.max_cantilever_m,
        )
    })
}

/// Blocks, rails and (when the face has a support grid) fixations of a stored array.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrayHardware {
    pub structure: Structure,
    pub module_ids: Vec<ModuleId>,
    pub rails: RailLayout,
    pub fixations: Option<Fixations>,
}

/// Mounting hardware of an array from its stored modules; `None` when its mounting system
/// has no rail rules. `fixation_every`: on a rafter grid, fixations on every n-th rafter
/// (from the statics); ignored on standing seams (one clamp per seam).
pub fn hardware(
    face: &RoofFace,
    array: &PanelArray,
    modules: &[ModuleInstance],
    fixation_every: u32,
) -> Result<Option<ArrayHardware>, LayoutError> {
    let Some((params, edge, cantilever)) = rail_params(array) else {
        return Ok(None);
    };
    let (structure, module_ids) = structure(array, modules)?;
    let cells: Vec<Cell> = module_ids
        .iter()
        .filter_map(|id| modules.iter().find(|m| m.id == *id))
        .map(|m| Cell {
            row: m.row,
            col: m.col,
            centre: Vec2::new(m.u, m.v),
        })
        .collect();
    let rails = cross_rails(&cells, &grid_spec(array), &structure, &params);
    let fixations = support_lines(face).map(|lines| {
        let every = match lines.kind {
            SupportKind::Rafters => fixation_every,
            SupportKind::StandingSeam => 1,
        };
        fixations(&rails, &lines, edge, cantilever, every)
    });
    Ok(Some(ArrayHardware {
        structure,
        module_ids,
        rails,
        fixations,
    }))
}
