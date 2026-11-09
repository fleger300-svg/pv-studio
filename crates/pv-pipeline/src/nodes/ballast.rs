//! Ballast of an aerodynamic east-west system, shared over the mounting blocks.
//!
//! An east-west system is two arrays (+90° / −90°) linked by `paired_with`; a pair is the
//! module of one half and the module of the other half in the same grid cell. The ballast
//! is computed once, on the half with the smaller id; the other half reports `None`.

use std::collections::HashMap;

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::command::keys;
use pv_model::{CommandError, ModuleId, ModuleState};
use pv_statics::ballast::{
    aero_pair_ballast, share_ballast_on_structure, BallastCase, GroupBallast,
};
use pv_statics::{wind, RoofZone};
use serde::Serialize;

use super::geometry::Roof;
use super::layout::StructureNode;
use super::loads::Site;
use super::zones::{severity, ModuleZones};
use super::{array_of, kind};
use crate::convert::{ballast_inputs, factors};
use crate::error::statics_error;
use crate::inputs::{ArrayIn, ModulesIn, MountingIn};

/// Ballast need of one pair.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairNeed {
    pub module: ModuleId,
    /// Module of the other half in the same cell (`None`: incomplete pair, computed alone).
    pub partner: Option<ModuleId>,
    /// Most severe zone of the two modules.
    pub zone: RoofZone,
    /// Required total stabilising mass m_req (governing check) [kg].
    pub required_mass_kg: f64,
    pub governing: BallastCase,
    /// Blocks if the pair were not linked to any other.
    pub blocks_alone: u32,
}

/// Ballast of an east-west system.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BallastPlan {
    pub pairs: Vec<PairNeed>,
    /// One group per mounting block (expansion joints split them).
    pub groups: Vec<GroupBallast>,
    pub total_blocks: u32,
}

kind!(
    /// Ballast of an east-west field; `None` for other systems and on the second half.
    Ballast,
    "array.ballast",
    Option<BallastPlan>
);

impl Derived for Ballast {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Option<BallastPlan>, GraphError> {
        let id = array_of(i)?;
        let mounting = ctx.get::<MountingIn>(i)?;
        let Some((m, spec)) = mounting
            .as_ref()
            .as_ref()
            .and_then(|m| m.ballast.as_ref().map(|b| (m, b)))
        else {
            return Ok(None);
        };
        let array = ctx.get::<ArrayIn>(i)?;
        let Some(partner) = array.layout.paired_with else {
            return Err(CommandError::new(keys::STATICS_EAST_WEST_UNPAIRED).into());
        };
        if partner < id {
            return Ok(None);
        }
        let other = Instance::Array(partner);
        let roof = ctx.get::<Roof>(Instance::Face(array.roof_face_id))?;
        let site = ctx.get::<Site>(Instance::Building(roof.building))?;
        let field = ctx.get::<StructureNode>(i)?;
        let own_modules = ctx.get::<ModulesIn>(i)?;
        let other_modules = ctx.get::<ModulesIn>(other)?;
        let own_zones = ctx.get::<ModuleZones>(i)?;
        let other_zones = ctx.get::<ModuleZones>(other)?;

        let qp = wind::peak_pressure(&site)
            .map_err(|e| statics_error(&e))?
            .peak_pressure
            .value;
        let factors = factors(&site, m);
        let k_fi = site.consequence_class.importance_factor();
        let (table, pair, surface, block) = ballast_inputs(spec, &array);

        let zone_of: HashMap<ModuleId, RoofZone> = own_zones
            .iter()
            .chain(other_zones.iter())
            .map(|z| (z.module, z.zone))
            .collect();
        let cell_of: HashMap<ModuleId, (u32, u32)> =
            own_modules.iter().map(|m| (m.id, (m.row, m.col))).collect();
        let partner_at: HashMap<(u32, u32), ModuleId> = other_modules
            .iter()
            .filter(|m| m.state == ModuleState::Active)
            .map(|m| ((m.row, m.col), m.id))
            .collect();

        let mut by_zone: HashMap<RoofZone, (f64, BallastCase, u32)> = HashMap::new();
        let mut pairs = Vec::with_capacity(field.module_ids.len());
        for &module in &field.module_ids {
            let partner = cell_of
                .get(&module)
                .and_then(|c| partner_at.get(c))
                .copied();
            let zone = [Some(module), partner]
                .into_iter()
                .flatten()
                .filter_map(|id| zone_of.get(&id).copied())
                .max_by_key(|z| severity(*z))
                .unwrap_or(RoofZone::Interior);
            let need = match by_zone.get(&zone) {
                Some(n) => *n,
                None => {
                    let r = aero_pair_ballast(
                        &table, zone, qp, &pair, &surface, &factors, k_fi, &block,
                    )
                    .map_err(|e| statics_error(&e))?;
                    let n = (
                        r.uplift
                            .required_mass
                            .value
                            .max(r.sliding.required_mass.value),
                        r.governing,
                        r.blocks,
                    );
                    by_zone.insert(zone, n);
                    n
                }
            };
            pairs.push(PairNeed {
                module,
                partner,
                zone,
                required_mass_kg: need.0,
                governing: need.1,
                blocks_alone: need.2,
            });
        }
        let required: Vec<f64> = pairs.iter().map(|p| p.required_mass_kg).collect();
        let own = vec![pair.own_mass_kg; pairs.len()];
        let groups =
            share_ballast_on_structure(&field.structure, &required, &own, block.unit_mass_kg)
                .map_err(|e| statics_error(&e))?;
        let total_blocks = groups.iter().map(|g| g.blocks).sum();
        Ok(Some(BallastPlan {
            pairs,
            groups,
            total_blocks,
        }))
    }
}
