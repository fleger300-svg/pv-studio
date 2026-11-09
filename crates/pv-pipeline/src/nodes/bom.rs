//! Bill of materials: the counters of each field (from its layout), then the project's
//! tables (`pv-bom`).
//!
//! Cut-off: rail lengths are rounded to the millimetre (as K2 prints them), so a module moved
//! inside its block leaves the counters unchanged and the bill of materials is not
//! recomputed.

use pv_bom::{compute, Bom, FaceRef, FieldCounts, TopRail};
use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::{Driver, ModuleState};

use super::ballast::Ballast;
use super::layout::Hardware;
use super::{array_of, kind};
use crate::error::bom_error;
use crate::inputs::{ArrayIn, FaceIn, ModulesIn, MountingIn, PricingIn, ProjectIndexIn};

fn mm(length_m: f64) -> f64 {
    (length_m * 1000.0).round() / 1000.0
}

kind!(
    /// Counters of a field for its bill of materials.
    FieldCountsNode,
    "array.bom_counts",
    FieldCounts
);

impl Derived for FieldCountsNode {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<FieldCounts, GraphError> {
        array_of(i)?;
        let array = ctx.get::<ArrayIn>(i)?;
        let face = ctx.get::<FaceIn>(Instance::Face(array.roof_face_id))?;
        let mounting = ctx.get::<MountingIn>(i)?;
        let parts = mounting
            .as_ref()
            .as_ref()
            .map(|m| m.parts.clone())
            .unwrap_or_default();
        let mut counts = FieldCounts {
            face: FaceRef {
                id: face.id.0.to_string(),
                name: face.name.clone(),
            },
            modules: 0,
            top_rails: Vec::new(),
            bottom_rails: Vec::new(),
            climbers: 0,
            fixations: 0,
            ballast_blocks: 0,
            parts,
        };
        // Read only what the rules count (dynamic dependencies): a field without parts does
        // not depend on its layout at all.
        let uses = |f: fn(&Driver) -> bool| counts.parts.iter().any(|p| f(&p.driver));
        let rails = uses(|d| {
            matches!(
                d,
                Driver::Fixation
                    | Driver::Climber
                    | Driver::TopRailEnd
                    | Driver::TopRailInnerJoint
                    | Driver::RailBars { .. }
                    | Driver::RailJoint { .. }
            )
        });
        let modules = uses(|d| matches!(d, Driver::Module));
        let blocks = uses(|d| matches!(d, Driver::BallastBlock));
        if rails {
            if let Some(hw) = ctx.get::<Hardware>(i)?.as_ref() {
                counts.top_rails = hw
                    .rails
                    .top
                    .iter()
                    .zip(&hw.rails.top_modules)
                    .map(|(r, &n)| TopRail {
                        length_m: mm(r.length()),
                        modules: n,
                    })
                    .collect();
                // Bottom rails as fixed: possibly lengthened to reach a support line.
                let bottom = hw
                    .fixations
                    .as_ref()
                    .map_or(&hw.rails.bottom, |f| &f.bottom);
                counts.bottom_rails = bottom.iter().map(|r| mm(r.length())).collect();
                counts.climbers = u32::try_from(hw.rails.climbers.len()).unwrap_or(u32::MAX);
                counts.fixations = hw
                    .fixations
                    .as_ref()
                    .map_or(0, |f| u32::try_from(f.count()).unwrap_or(u32::MAX));
            }
        }
        if modules {
            let ms = ctx.get::<ModulesIn>(i)?;
            counts.modules =
                u32::try_from(ms.iter().filter(|m| m.state == ModuleState::Active).count())
                    .unwrap_or(u32::MAX);
        }
        if blocks {
            counts.ballast_blocks = ctx
                .get::<Ballast>(i)?
                .as_ref()
                .as_ref()
                .map_or(0, |b| b.total_blocks);
        }
        Ok(counts)
    }
}

kind!(
    /// Bill of materials of the project (watched: recomputed automatically).
    ProjectBom,
    "project.bom",
    Bom
);

impl Derived for ProjectBom {
    fn compute(ctx: &mut Ctx<'_>, _: Instance) -> Result<Bom, GraphError> {
        let index = ctx.get::<ProjectIndexIn>(Instance::Project)?;
        let pricing = ctx.get::<PricingIn>(Instance::Project)?;
        let mut fields = Vec::with_capacity(index.arrays.len());
        for (a, _) in &index.arrays {
            fields.push((*ctx.get::<FieldCountsNode>(Instance::Array(*a))?).clone());
        }
        // The Total tab's title is translated by the front end.
        compute(&fields, &pricing, "total").map_err(|e| bom_error(&e).into())
    }
}
