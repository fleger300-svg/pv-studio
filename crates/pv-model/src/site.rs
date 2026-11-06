//! Site data of a building, for the statics (SIA 261 snow and wind).
//!
//! Stored per building: on one site, buildings may differ in height and terrain category.
//! Plain persisted data; `pv-statics` owns the formulas (`pv-pipeline` maps these types onto
//! its `SiteInput`). The reference height of the wind profile is the building's
//! `ridge_height_m`, not stored twice.

use serde::{Deserialize, Serialize};
use specta::Type;

/// SIA 261 terrain category (tab. 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum TerrainCategory {
    II,
    IIa,
    III,
    IV,
}

/// Snow exposure (coefficient C_e).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SnowExposure {
    Windswept,
    Normal,
    Sheltered,
}

/// Consequence class (SIA 260), sets the importance factor K_FI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ConsequenceClass {
    CC1,
    CC2,
    CC3,
}

/// How the service life adjusts the characteristic loads.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ServiceLifeModel {
    /// SN EN 1991-1-3 annex D and SN EN 1991-1-4 §4.2 (c_prob).
    En1991,
    /// Factors fixed by the engineer.
    Explicit { snow: f64, wind: f64 },
}

/// Site data of a building.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SiteParams {
    /// Terrain altitude above sea level [m].
    pub altitude_m: f64,
    /// Correction from the SIA 261 snow map added to the altitude (reference altitude h₀) [m].
    pub snow_altitude_correction_m: f64,
    /// Reference peak velocity pressure q_p0 (SIA 261 annex E) [kN/m²].
    pub wind_reference_pressure_kn_m2: f64,
    pub terrain: TerrainCategory,
    pub snow_exposure: SnowExposure,
    /// Thermal coefficient C_t [-].
    pub thermal_coefficient: f64,
    /// Design service life [years].
    pub service_life_years: f64,
    pub consequence_class: ConsequenceClass,
    pub service_life_model: ServiceLifeModel,
}

impl SiteParams {
    /// Defaults for a new building: 50 years, CC2, normal exposure, C_t = 1. Altitude and
    /// q_p0 come from the project position (to be filled by the site step).
    #[must_use]
    pub fn new(altitude_m: f64, wind_reference_pressure_kn_m2: f64) -> Self {
        Self {
            altitude_m,
            snow_altitude_correction_m: 0.0,
            wind_reference_pressure_kn_m2,
            terrain: TerrainCategory::III,
            snow_exposure: SnowExposure::Normal,
            thermal_coefficient: 1.0,
            service_life_years: 50.0,
            consequence_class: ConsequenceClass::CC2,
            service_life_model: ServiceLifeModel::En1991,
        }
    }
}
