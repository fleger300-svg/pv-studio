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
use super::components::MountingNode;
use super::layout::{Hardware, StructureNode};
use super::{array_of, kind};
use crate::error::bom_error;
use crate::inputs::{ArrayIn, BuildingIn, FaceIn, ModulesIn, PricingIn, ProjectIndexIn};

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
        // Tab title as K2 names it: building and roof face (« Bât 9A Toiture Sud »).
        let building = ctx.get::<BuildingIn>(Instance::Building(face.building_id))?;
        let mounting = ctx.get::<MountingNode>(i)?;
        let parts = mounting
            .as_ref()
            .as_ref()
            .map(|m| m.parts.clone())
            .unwrap_or_default();
        let mut counts = FieldCounts {
            face: FaceRef {
                id: face.id.0.to_string(),
                name: format!("{} · {}", building.name, face.name),
            },
            modules: 0,
            top_rails: Vec::new(),
            clamp_rails: Vec::new(),
            bottom_rails: Vec::new(),
            climbers: 0,
            fixations: 0,
            ballast_blocks: 0,
            pair_runs: Vec::new(),
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
        let runs = uses(|d| matches!(d, Driver::PairRun { .. }));
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
                // One-layer systems carry the modules on the bottom rails.
                counts.clamp_rails = if hw.rails.bottom_modules.is_empty() {
                    hw.rails.top_modules.clone()
                } else {
                    hw.rails.bottom_modules.clone()
                };
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
        if runs {
            counts.pair_runs = pair_runs(ctx, i, &array)?;
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

/// Pairs of each continuous east-west run of a paired field, counted on the half with the
/// smaller id (like the ballast; the other half counts none): cells of either half in the
/// same row and adjacent columns, split where the mounting block of the field changes
/// (expansion joint).
fn pair_runs(
    ctx: &mut Ctx<'_>,
    i: Instance,
    array: &pv_model::PanelArray,
) -> Result<Vec<u32>, GraphError> {
    use std::collections::{BTreeMap, HashMap};
    let Some(partner) = array.layout.paired_with else {
        return Ok(Vec::new());
    };
    if partner < array.id {
        return Ok(Vec::new());
    }
    let own = ctx.get::<ModulesIn>(i)?;
    let other = ctx.get::<ModulesIn>(Instance::Array(partner))?;
    let field = ctx.get::<StructureNode>(i)?;
    let block_of: HashMap<pv_model::ModuleId, usize> = field
        .module_ids
        .iter()
        .zip(&field.structure.block_of)
        .map(|(id, &b)| (*id, b))
        .collect();
    // (row, col) → block (None for a cell only the other half has).
    let mut cells: BTreeMap<(u32, u32), Option<usize>> = BTreeMap::new();
    for m in own.iter().filter(|m| m.state == ModuleState::Active) {
        cells.insert((m.row, m.col), block_of.get(&m.id).copied());
    }
    for m in other.iter().filter(|m| m.state == ModuleState::Active) {
        cells.entry((m.row, m.col)).or_insert(None);
    }
    let mut runs: Vec<u32> = Vec::new();
    let mut prev: Option<((u32, u32), Option<usize>)> = None;
    for (&(row, col), &block) in &cells {
        let continues = prev.is_some_and(|((r, c), b)| {
            r == row && c + 1 == col && (b.is_none() || block.is_none() || b == block)
        });
        match runs.last_mut() {
            Some(n) if continues => *n += 1,
            _ => runs.push(1),
        }
        prev = Some((
            (row, col),
            block.or(prev.and_then(|p| p.1).filter(|_| continues)),
        ));
    }
    Ok(runs)
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
