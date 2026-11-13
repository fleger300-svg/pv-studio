//! Load descent of a cross-rail field: rail checks and fixation spacing per zone.

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::command::keys;
use pv_model::{CommandError, RailLayers};
use pv_statics::descent::{descend, CrossRailSystem, DescentResult, SupportGrid, ZoneSurfaceLoads};
use pv_statics::RoofZone;
use serde::Serialize;

use super::components::MountingNode;
use super::layout::Hardware;
use super::loads::ZoneLoads;
use super::{array_of, kind};
use crate::convert::{fixation, rail_section, support_grid};
use crate::error::statics_error;
use crate::inputs::{ArrayIn, FaceIn};

/// Descent of one zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneDescent {
    pub zone: RoofZone,
    pub result: DescentResult,
}

kind!(
    /// Rail and fixation checks per zone; `None` when the field is not a cross-rail system.
    Descent,
    "array.descent",
    Option<Vec<ZoneDescent>>
);

/// Cross-rail system of a field for the statics, with its support grid; `None` when the
/// field is not a rail system.
fn rail_system(
    ctx: &mut Ctx<'_>,
    i: Instance,
) -> Result<Option<(CrossRailSystem, SupportGrid)>, GraphError> {
    let mounting = ctx.get::<MountingNode>(i)?;
    let Some(m) = mounting.as_ref() else {
        return Err(CommandError::new(keys::STATICS_MOUNTING_MISSING).into());
    };
    let Some(rails) = &m.rails else {
        return Ok(None);
    };
    let array = ctx.get::<ArrayIn>(i)?;
    let Some(spec) = array.layout.rails else {
        return Ok(None);
    };
    let face = ctx.get::<FaceIn>(Instance::Face(array.roof_face_id))?;
    let grid = support_grid(&face)
        .ok_or_else(|| GraphError::from(CommandError::new(keys::STATICS_NO_SUPPORT_GRID)))?;
    let single_layer = spec.layers == RailLayers::One;
    let (footprint_u, footprint_v) = array.half_footprint();
    // Width carried by one module-carrying rail: the module dimension across it, shared by
    // the rails of a module.
    let across = if single_layer {
        footprint_v
    } else {
        footprint_u
    };
    let system = CrossRailSystem {
        top_rail: rail_section(&rails.top_rail),
        bottom_rail: rail_section(&rails.bottom_rail),
        top_rail_tributary_width_m: 2.0 * across / f64::from(spec.per_column.max(1)),
        bottom_rail_spacing_m: spec.bottom_spacing_m,
        support_grid: grid,
        actual_fixation_spacing_m: None,
        fixation: rails.fixation.as_ref().map(fixation),
        single_layer,
    };
    Ok(Some((system, grid)))
}

fn descend_zones(
    ctx: &mut Ctx<'_>,
    i: Instance,
    system: &CrossRailSystem,
) -> Result<Vec<ZoneDescent>, GraphError> {
    let loads = ctx.get::<ZoneLoads>(i)?;
    loads
        .zones
        .iter()
        .map(|zl| {
            descend(system, &ZoneSurfaceLoads::from_envelopes(&zl.uls, &zl.sls))
                .map(|result| ZoneDescent {
                    zone: zl.zone,
                    result,
                })
                .map_err(|e| statics_error(&e).into())
        })
        .collect()
}

impl Derived for Descent {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Option<Vec<ZoneDescent>>, GraphError> {
        array_of(i)?;
        let Some((mut system, grid)) = rail_system(ctx, i)? else {
            return Ok(None);
        };
        // Rafters: the largest spacing that actually occurs on the laid-out bottom rails.
        // Only read then (dynamic dependency): on standing seams, moving modules does not
        // touch the descent.
        system.actual_fixation_spacing_m = match grid {
            SupportGrid::Rafters { .. } => largest_fixation_spacing(&*ctx.get::<Hardware>(i)?),
            SupportGrid::StandingSeam { .. } => None,
        };
        descend_zones(ctx, i, &system).map(Some)
    }
}

kind!(
    /// On a rafter grid, fixations go on every n-th rafter: the smallest n the statics allow
    /// over the field's zones (one spacing for the whole field, governed by its most loaded
    /// zone). 1 on standing seams, for non-rail systems, and when the statics cannot be
    /// computed (the statics panel reports why): never fewer fixations than allowed.
    FixationEvery,
    "array.fixation_every",
    u32
);

impl Derived for FixationEvery {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<u32, GraphError> {
        array_of(i)?;
        let Ok(Some((system, SupportGrid::Rafters { spacing_m }))) = rail_system(ctx, i) else {
            return Ok(1);
        };
        let Ok(zones) = descend_zones(ctx, i, &system) else {
            return Ok(1);
        };
        let n = zones
            .iter()
            .map(|z| (z.result.fixation_spacing.value / spacing_m + 1e-9).floor())
            .fold(f64::INFINITY, f64::min);
        Ok(if n.is_finite() && n >= 1.0 {
            n.min(f64::from(u32::MAX)) as u32
        } else {
            1
        })
    }
}

fn largest_fixation_spacing(hw: &Option<pv_layout::model::ArrayHardware>) -> Option<f64> {
    let fixations = hw.as_ref()?.fixations.as_ref()?;
    fixations
        .positions
        .iter()
        .flat_map(|p| {
            let mut p = p.clone();
            p.sort_by(f64::total_cmp);
            p.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>()
        })
        .max_by(f64::total_cmp)
}
