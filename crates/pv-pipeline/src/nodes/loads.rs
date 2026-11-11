//! Site and design loads of a field (SIA 261 snow and wind, SIA 260 combinations).

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::command::keys;
use pv_model::CommandError;
use pv_statics::{analyze_zones, SiteInput, ZoneLoadCase, ZoneLoadResult};

use super::components::MountingNode;
use super::geometry::Roof;
use super::zones::ZoneSet;
use super::{array_of, building_of, kind};
use crate::convert::{cp_source, dead_load, factors, site_input};
use crate::error::statics_error;
use crate::inputs::{ArrayIn, BuildingIn, SiteIn};

kind!(
    /// Site of a building as the statics engine takes it (validated).
    Site,
    "building.site",
    SiteInput
);

impl Derived for Site {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<SiteInput, GraphError> {
        building_of(i)?;
        let site = ctx.get::<SiteIn>(i)?;
        let Some(site) = site.as_ref() else {
            return Err(CommandError::new(keys::STATICS_SITE_MISSING).into());
        };
        let building = ctx.get::<BuildingIn>(i)?;
        let input = site_input(site, &building);
        input.validate().map_err(|e| statics_error(&e))?;
        Ok(input)
    }
}

kind!(
    /// Snow, wind and design loads for every zone of a field.
    ZoneLoads,
    "array.zone_loads",
    ZoneLoadResult
);

impl Derived for ZoneLoads {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<ZoneLoadResult, GraphError> {
        array_of(i)?;
        let mounting = ctx.get::<MountingNode>(i)?;
        let Some(m) = mounting.as_ref() else {
            return Err(CommandError::new(keys::STATICS_MOUNTING_MISSING).into());
        };
        let array = ctx.get::<ArrayIn>(i)?;
        let roof = ctx.get::<Roof>(Instance::Face(array.roof_face_id))?;
        let site = ctx.get::<Site>(Instance::Building(roof.building))?;
        let zones = ctx.get::<ZoneSet>(i)?;
        let case = ZoneLoadCase {
            site: (*site).clone(),
            roof: roof.roof,
            dead_load: dead_load(m, &array),
            factors: factors(&site, m),
            cp_source: cp_source(&m.cp),
            zones: zones.to_vec(),
            loaded_area_m2: m.loaded_area_m2,
        };
        analyze_zones(&case).map_err(|e| statics_error(&e).into())
    }
}
