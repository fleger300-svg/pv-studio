//! PV Studio statics engine — pre-dimensioning according to SIA 261 / SIA 260.
//!
//! Chain implemented so far (architecture doc, « Module statique »):
//!
//! 1. [`trace`]: every result is a [`Traced`] value with formula, clause, inputs and
//!    assumptions (ADR-004), and inputs carry a [`fingerprint`] for the result cache.
//! 2. [`site`]: site and building data.
//! 3. [`snow`]: ground and roof snow load, service-life adjustment.
//! 4. [`wind`]: height profile, peak velocity pressure; [`cp`]: pressure coefficients per roof
//!    zone, normative or from a manufacturer.
//! 5. [`combos`]: dead load and SIA 260 combinations, split ⊥ / ∥ to the roof.
//!
//! 6. [`descent`]: load descent surface → top rail → bottom rail → fixation, rail and
//!    fixation checks, maximum spacings, fixation spacing on the support grid, counts.
//! 7. [`ballast`]: flat-roof ballast against uplift and sliding, aerodynamic east-west
//!    systems, sharing within mechanically linked positions, rounding to whole blocks.
//!
//! [`zones`]: wind zones of the whole face, assigned to each field by position.
//!
//! [`analyze_zones`] runs steps 2–5 for one field; its envelopes feed [`descent::descend`].
//!
//! The results are a **pre-dimensioning**: every report and UI view showing them must also
//! show [`disclaimer`].
//!
//! Units: kN, kN/m², m, degrees.

pub mod ballast;
pub mod combos;
pub mod cp;
pub mod descent;
pub mod site;
pub mod snow;
pub mod trace;
pub mod wind;
pub mod zones;

use serde::{Deserialize, Serialize};

pub use combos::{
    CombinationId, DeadLoad, DesignEnvelope, LimitState, PartialFactors, SurfaceActions,
};
pub use cp::{
    CpEntry, CpSource, ExplicitCpTable, ManufacturerCpTable, RoofShape, RoofZone, ZoneCp,
};
pub use site::{ConsequenceClass, ServiceLifeModel, SiteInput, SnowExposure, TerrainCategory};
pub use snow::SnowResult;
pub use trace::{fingerprint, Assumption, NormRef, Traced, Unit};
pub use wind::WindResult;

/// Errors of the statics engine. They carry the offending quantity so that the UI can point
/// at the field to fix.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StaticsError {
    #[error("{quantity} = {value} is outside the valid range [{min}, {max}]")]
    OutOfRange {
        quantity: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },
    #[error("no pressure coefficient for zone {zone:?}")]
    MissingCoefficient { zone: RoofZone },
    #[error("not supported yet: {what}")]
    Unsupported { what: String },
}

impl StaticsError {
    pub(crate) fn check_range(
        quantity: &'static str,
        value: f64,
        min: f64,
        max: f64,
    ) -> Result<(), StaticsError> {
        if value.is_finite() && (min..=max).contains(&value) {
            Ok(())
        } else {
            Err(StaticsError::OutOfRange {
                quantity,
                value,
                min,
                max,
            })
        }
    }
}

/// Annual exceedance probability giving, over a service life of `years`, the same probability
/// of exceedance as a 50-year return period over 50 years: p = 1 − 0.98^(50/t_L).
pub(crate) fn lifetime_annual_probability(years: f64) -> f64 {
    1.0 - 0.98_f64.powf(50.0 / years)
}

/// Language of the mandatory disclaimer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Fr,
    En,
    De,
}

/// Message key of the mandatory pre-dimensioning disclaimer (ADR-004).
pub const DISCLAIMER_KEY: &str = "statics.disclaimer.pre_dimensioning";

/// Mandatory disclaimer shown next to every statics result and on every statics report page.
#[must_use]
pub fn disclaimer(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "Pré-dimensionnement, à faire valider pour exécution.",
        Lang::En => "Pre-dimensioning only, to be validated before execution.",
        Lang::De => "Vordimensionierung, vor der Ausführung zu prüfen.",
    }
}

/// Roof face data needed by the load chain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoofInput {
    pub shape: RoofShape,
    pub pitch_deg: f64,
}

/// Input of [`analyze_zones`]: one field (array) on one roof face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneLoadCase {
    pub site: SiteInput,
    pub roof: RoofInput,
    pub dead_load: DeadLoad,
    #[serde(default)]
    pub factors: PartialFactors,
    #[serde(default)]
    pub cp_source: CpSource,
    /// Zones the field touches.
    pub zones: Vec<RoofZone>,
    /// Loaded area for the pressure coefficients [m²]: the module area to size modules,
    /// the tributary area of the element to size the mounting system (K2 uses 10 m²).
    pub loaded_area_m2: f64,
}

/// Wind and design loads of one zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneLoads {
    pub zone: RoofZone,
    pub cp: ZoneCp,
    /// Characteristic wind pressure q_p·c_pe,+ [kN/m²].
    pub wind_pressure: Traced,
    /// Characteristic wind suction q_p·c_pe,− [kN/m²].
    pub wind_suction: Traced,
    pub uls: DesignEnvelope,
    pub sls: DesignEnvelope,
}

/// Result of [`analyze_zones`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneLoadResult {
    /// Fingerprint of the [`ZoneLoadCase`] that produced this result.
    pub input_fingerprint: u64,
    pub disclaimer_key: &'static str,
    pub snow: SnowResult,
    pub wind: WindResult,
    pub dead_load: combos::DeadLoadResult,
    pub importance_factor: Traced,
    pub zones: Vec<ZoneLoads>,
}

/// Snow, wind and combined design loads for every zone of a field.
pub fn analyze_zones(case: &ZoneLoadCase) -> Result<ZoneLoadResult, StaticsError> {
    let snow = snow::roof_snow(&case.site, case.roof.pitch_deg)?;
    let wind = wind::peak_pressure(&case.site)?;
    let dead_load = case.dead_load.evaluate()?;
    let k_fi = case.site.consequence_class.importance_factor();
    let importance_factor = Traced::new("K_FI", k_fi, Unit::Dimensionless)
        .formula("K_FI from consequence class")
        .reference(NormRef::new("SN EN 1990", "annex B, tab. B3"))
        .assume(
            "statics.assumption.consequence_class",
            format!("Consequence class {:?}", case.site.consequence_class),
        );
    let qp = &wind.peak_pressure;

    let zones = case
        .zones
        .iter()
        .map(|&zone| {
            let cp = case.cp_source.zone_coefficients(
                case.roof.shape,
                case.roof.pitch_deg,
                zone,
                case.loaded_area_m2,
            )?;
            let wind_pressure = Traced::new(
                "w_k,+",
                qp.value * cp.pressure.value,
                Unit::KiloNewtonPerSquareMetre,
            )
            .formula("w_k = q_p·c_pe,+")
            .input_from(qp)
            .input_from(&cp.pressure);
            let wind_suction = Traced::new(
                "w_k,−",
                qp.value * cp.suction.value,
                Unit::KiloNewtonPerSquareMetre,
            )
            .formula("w_k = q_p·c_pe,−")
            .input_from(qp)
            .input_from(&cp.suction);
            let actions = SurfaceActions {
                pitch_deg: case.roof.pitch_deg,
                dead: dead_load.load.value,
                snow: snow.roof_load.value,
                wind_pressure: wind_pressure.value,
                wind_suction: wind_suction.value,
            };
            Ok(ZoneLoads {
                zone,
                cp,
                wind_pressure,
                wind_suction,
                uls: combos::combine(&actions, &case.factors, k_fi, LimitState::Uls),
                sls: combos::combine(&actions, &case.factors, k_fi, LimitState::Sls),
            })
        })
        .collect::<Result<Vec<_>, StaticsError>>()?;

    Ok(ZoneLoadResult {
        input_fingerprint: fingerprint(case),
        disclaimer_key: DISCLAIMER_KEY,
        snow,
        wind,
        dead_load,
        importance_factor,
        zones,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn site_fixture(service_life_years: f64) -> SiteInput {
        SiteInput {
            altitude_m: 450.0,
            snow_altitude_correction_m: 0.0,
            wind_reference_pressure_kn_m2: 1.0,
            terrain: TerrainCategory::III,
            building_height_m: 10.0,
            snow_exposure: SnowExposure::Normal,
            thermal_coefficient: 1.0,
            service_life_years,
            consequence_class: ConsequenceClass::CC2,
            service_life_model: ServiceLifeModel::En1991,
        }
    }

    #[test]
    fn lifetime_probability_50_years_is_two_percent() {
        assert!((lifetime_annual_probability(50.0) - 0.02).abs() < 1e-12);
        assert!((lifetime_annual_probability(25.0) - 0.0396).abs() < 1e-12);
    }

    #[test]
    fn invalid_site_is_rejected() {
        let mut site = site_fixture(25.0);
        site.building_height_m = -3.0;
        assert!(matches!(
            wind::peak_pressure(&site),
            Err(StaticsError::OutOfRange {
                quantity: "building_height_m",
                ..
            })
        ));
    }

    #[test]
    fn disclaimer_exists_in_all_languages() {
        for lang in [Lang::Fr, Lang::En, Lang::De] {
            assert!(!disclaimer(lang).is_empty());
        }
    }
}
