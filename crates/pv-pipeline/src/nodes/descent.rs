//! Load descent of a cross-rail field: rail checks and fixation spacing per zone.

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::command::keys;
use pv_model::CommandError;
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

impl Derived for Descent {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Option<Vec<ZoneDescent>>, GraphError> {
        array_of(i)?;
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
        // Rafters: the largest spacing that actually occurs on the laid-out bottom rails.
        // Only read then (dynamic dependency): on standing seams, moving modules does not
        // touch the descent.
        let actual = match grid {
            SupportGrid::Rafters { .. } => largest_fixation_spacing(&*ctx.get::<Hardware>(i)?),
            SupportGrid::StandingSeam { .. } => None,
        };
        let loads = ctx.get::<ZoneLoads>(i)?;
        let (footprint_u, _) = array.half_footprint();
        let system = CrossRailSystem {
            top_rail: rail_section(&rails.top_rail),
            bottom_rail: rail_section(&rails.bottom_rail),
            top_rail_tributary_width_m: 2.0 * footprint_u / f64::from(spec.per_column.max(1)),
            bottom_rail_spacing_m: spec.bottom_spacing_m,
            support_grid: grid,
            actual_fixation_spacing_m: actual,
            fixation: rails.fixation.as_ref().map(fixation),
        };
        loads
            .zones
            .iter()
            .map(|zl| {
                descend(&system, &ZoneSurfaceLoads::from_envelopes(&zl.uls, &zl.sls))
                    .map(|result| ZoneDescent {
                        zone: zl.zone,
                        result,
                    })
                    .map_err(|e| statics_error(&e).into())
            })
            .collect::<Result<Vec<_>, GraphError>>()
            .map(Some)
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
