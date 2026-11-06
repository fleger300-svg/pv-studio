//! Several arrays — possibly with different mounting systems — on one roof face
//! (architecture: « Mixage des systèmes sur un même pan »).
//!
//! Arrays are laid out in order. Each one sees the mounting blocks already placed on the face
//! as keep-out areas, with a clearance equal to the larger of the two arrays'
//! `min_distance_to_other_arrays_m`, so fields never overlap and keep their distance. An
//! east-west system is given as its two `pv-model` arrays (+90° / −90°) and laid out as pairs.

use pv_geo::Polygon;
use pv_model::{ObstacleKind, PanelArray, RoofFace};

use crate::blocks::Structure;
use crate::grid::{fill, fill_east_west, Cell, EastWestSpec, Region, Search};
use crate::joints::JointRules;
use crate::model::{field_boundary, grid_spec, setback_rules, ArrayLayout};
use crate::roof::{Obstacle, RoofOutline};
use crate::setbacks::BuildableArea;
use crate::{build_structure, LayoutError};

/// What to lay out on the face, in priority order.
#[derive(Debug, Clone, Copy)]
pub enum FieldPlan<'a> {
    Single(&'a PanelArray),
    /// East-west pairs: the `+90°` and the `−90°` array.
    EastWest(&'a PanelArray, &'a PanelArray),
}

impl FieldPlan<'_> {
    fn primary(&self) -> &PanelArray {
        match self {
            FieldPlan::Single(a) | FieldPlan::EastWest(a, _) => a,
        }
    }
}

/// Layout of one plan entry: one array, or the two arrays of an east-west system.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldLayout {
    Single(ArrayLayout, Structure),
    EastWest {
        plus90: ArrayLayout,
        minus90: ArrayLayout,
        pairs: Structure,
    },
}

fn keep_outs(structure: &Structure, clearance: f64, name: &str) -> Vec<Obstacle> {
    structure
        .blocks
        .iter()
        .map(|b| Obstacle {
            name: format!("{name} block {}", b.id + 1),
            kind: ObstacleKind::KeepOut,
            footprint: Polygon::rectangle(b.lo.u, b.lo.v, b.hi.u, b.hi.v),
            height_m: 0.0,
            clearance_m: Some(clearance),
        })
        .collect()
}

/// Lays out every plan entry on the face, in order.
pub fn layout_face(
    face: &RoofFace,
    plans: &[FieldPlan<'_>],
    search: Search,
) -> Result<Vec<FieldLayout>, LayoutError> {
    let mut placed: Vec<(Structure, f64)> = Vec::new();
    let mut out = Vec::new();
    for plan in plans {
        let primary = plan.primary();
        if primary.roof_face_id != face.id {
            return Err(LayoutError::WrongFace);
        }
        let own_gap = primary.layout.min_distance_to_other_arrays_m;
        let mut roof = RoofOutline::from_face(face)?;
        for (i, (s, gap)) in placed.iter().enumerate() {
            for ob in keep_outs(s, own_gap.max(*gap), &format!("field {}", i + 1)) {
                roof = roof.with_obstacle(ob)?;
            }
        }
        let area = BuildableArea::new(roof, &setback_rules(primary));
        let boundary = field_boundary(primary)?;
        let region = Region {
            area: &area,
            boundary: boundary.as_ref(),
        };
        let rules = JointRules::from_params(&primary.layout);
        match plan {
            FieldPlan::Single(a) => {
                let spec = grid_spec(a);
                spec.validate()?;
                let p = fill(region, spec, search);
                let s = build_structure(&p.cells, &spec, &rules)?;
                placed.push((s.clone(), own_gap));
                out.push(FieldLayout::Single(ArrayLayout::from_placement(&p), s));
            }
            FieldPlan::EastWest(plus90, minus90) => {
                if minus90.roof_face_id != face.id {
                    return Err(LayoutError::WrongFace);
                }
                let pair_pitch = plus90
                    .layout
                    .row_pitch_m
                    .ok_or(LayoutError::MissingPairPitch)?;
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
                crate::model::ew_pitch_check(&spec)?;
                let ew = fill_east_west(region, spec, search);
                let pairs = build_structure(&ew.pairs.cells, &ew.pairs.spec, &rules)?;
                placed.push((pairs.clone(), own_gap));
                let half = (spec.footprint_u + spec.peak_gap) / 2.0;
                let side = |cells: &[Cell], shift: f64| {
                    let mut l = ArrayLayout::from_placement(&ew.pairs);
                    l.grid.origin_u += shift;
                    l.cells = cells.to_vec();
                    l
                };
                out.push(FieldLayout::EastWest {
                    plus90: side(&ew.minus_u, -half),
                    minus90: side(&ew.plus_u, half),
                    pairs,
                });
            }
        }
    }
    Ok(out)
}
