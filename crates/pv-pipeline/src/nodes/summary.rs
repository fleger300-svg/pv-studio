//! What a field's statics decide: the result the application watches and sends to the UI.

use pv_graph::{Ctx, Derived, GraphError, Instance};
use pv_model::WindZone;
use serde::Serialize;
use specta::Type;

use super::ballast::Ballast;
use super::descent::Descent;
use super::layout::Hardware;
use super::zones::ZoneSet;
use super::{array_of, kind};
use crate::convert::wind_zone;

/// Decision of one zone of a cross-rail field.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ZoneDecision {
    pub zone: WindZone,
    /// Fixation spacing on the support grid [m].
    pub fixation_spacing_m: f64,
    /// Largest utilisation of the zone (rails bending and deflection, fixation).
    pub max_utilisation: f64,
}

/// Statics of one field.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldStaticsResult {
    pub zones: Vec<WindZone>,
    /// Cross-rail systems.
    pub decisions: Vec<ZoneDecision>,
    pub fixations: Option<u32>,
    pub climbers: Option<u32>,
    /// Ballasted systems (on the half that carries the computation).
    pub ballast_blocks: Option<u32>,
    pub max_utilisation: Option<f64>,
    /// Every utilisation ≤ 1.
    pub ok: bool,
}

kind!(
    /// Statics of a field (watched: recomputed automatically after every command).
    FieldStatics,
    "array.statics",
    FieldStaticsResult
);

impl Derived for FieldStatics {
    fn compute(ctx: &mut Ctx<'_>, i: Instance) -> Result<FieldStaticsResult, GraphError> {
        array_of(i)?;
        let zones = ctx.get::<ZoneSet>(i)?;
        let descent = ctx.get::<Descent>(i)?;
        let ballast = ctx.get::<Ballast>(i)?;
        let mut decisions = Vec::new();
        let (mut fixations, mut climbers) = (None, None);
        if let Some(zd) = descent.as_ref() {
            for d in zd {
                let r = &d.result;
                let mut u = [
                    r.top_rail.bending_utilisation.value,
                    r.top_rail.deflection_utilisation.value,
                    r.bottom_rail.bending_utilisation.value,
                    r.bottom_rail.deflection_utilisation.value,
                ]
                .into_iter()
                .fold(0.0_f64, f64::max);
                if let Some(fc) = &r.fixation_check {
                    u = u
                        .max(fc.perpendicular_utilisation.value)
                        .max(fc.parallel_utilisation.value);
                }
                decisions.push(ZoneDecision {
                    zone: wind_zone(d.zone),
                    fixation_spacing_m: r.fixation_spacing.value,
                    max_utilisation: u,
                });
            }
            if let Some(hw) = ctx.get::<Hardware>(i)?.as_ref() {
                climbers = Some(count(hw.rails.climbers.len()));
                fixations = hw.fixations.as_ref().map(|f| count(f.count()));
            }
        }
        let max_utilisation = decisions
            .iter()
            .map(|d| d.max_utilisation)
            .max_by(f64::total_cmp);
        Ok(FieldStaticsResult {
            zones: zones.iter().map(|z| wind_zone(*z)).collect(),
            decisions,
            fixations,
            climbers,
            ballast_blocks: ballast.as_ref().as_ref().map(|b| b.total_blocks),
            max_utilisation,
            ok: max_utilisation.is_none_or(|u| u <= 1.0),
        })
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
