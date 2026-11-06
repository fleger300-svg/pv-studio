//! Wind zones of the modules of a field, from the zones of the whole face.

use pv_geo::Vec2;
use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::{ModuleId, ModuleState};
use pv_statics::zones::zone_at;
use pv_statics::RoofZone;
use serde::Serialize;

use super::geometry::{Outline, ZoneGeom};
use super::{array_of, kind};
use crate::inputs::{ArrayIn, ModulesIn};

/// Zone of one active module.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ModuleZone {
    pub module: ModuleId,
    pub zone: RoofZone,
}

kind!(
    /// Zone of every active module (the zone its centre lies in). Cut-off: a module moved
    /// inside its zone leaves this unchanged.
    ModuleZones,
    "array.module_zones",
    Vec<ModuleZone>
);

impl Derived for ModuleZones {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Vec<ModuleZone>, GraphError> {
        array_of(i)?;
        let array = ctx.get::<ArrayIn>(i)?;
        let face = Instance::Face(array.roof_face_id);
        let outline = ctx.get::<Outline>(face)?;
        let geometry = ctx.get::<ZoneGeom>(face)?;
        let modules = ctx.get::<ModulesIn>(i)?;
        Ok(modules
            .iter()
            .filter(|m| m.state == ModuleState::Active)
            .map(|m| ModuleZone {
                module: m.id,
                zone: zone_at(&outline, &geometry, Vec2::new(m.u, m.v)),
            })
            .collect())
    }
}

kind!(
    /// Zones a field touches, in the order of `RoofZone::ALL`. Cut-off: the loads depend on
    /// this set only.
    ZoneSet,
    "array.zone_set",
    Vec<RoofZone>
);

impl Derived for ZoneSet {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<Vec<RoofZone>, GraphError> {
        let zones = ctx.get::<ModuleZones>(i)?;
        Ok(RoofZone::ALL
            .into_iter()
            .filter(|z| zones.iter().any(|m| m.zone == *z))
            .collect())
    }
}

/// Severity order of the zones (the most severe one wins when a pair straddles two).
#[must_use]
pub fn severity(z: RoofZone) -> u8 {
    match z {
        RoofZone::Interior => 0,
        RoofZone::Eave => 1,
        RoofZone::Ridge => 2,
        RoofZone::Verge => 3,
        RoofZone::EaveCorner => 4,
    }
}
