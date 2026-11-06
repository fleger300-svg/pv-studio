//! Site data: altitude, snow reference altitude, wind reference pressure, terrain, building.
//!
//! These are filled automatically from the project position (SIA 261 annexes D and E) and
//! can be overridden by the user. The engine only validates and uses them.

use serde::{Deserialize, Serialize};

use crate::trace::{NormRef, Traced, Unit};
use crate::StaticsError;

pub const SIA_261_ANNEX_D: NormRef = NormRef::new("SIA 261:2014", "Annexe D");
pub const SIA_261_TERRAIN: NormRef = NormRef::new("SIA 261:2014", "§6.2.1, tab. 4");

/// Terrain category (SIA 261 tab. 4). K2 labels: III = "Towns, open fields",
/// IV = "Large urban areas".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainCategory {
    /// Lake shores, flat open land.
    II,
    /// Large open plains.
    IIa,
    /// Villages, open countryside with obstacles.
    III,
    /// Large built-up urban areas.
    IV,
}

/// Parameters of the SIA 261 height profile for one terrain category.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TerrainParameters {
    /// Gradient height z_g [m].
    pub z_g: f64,
    /// Profile exponent α_r [-].
    pub alpha_r: f64,
    /// Minimum height z_min [m]; below it the profile is constant.
    pub z_min: f64,
}

impl TerrainCategory {
    #[must_use]
    pub fn parameters(self) -> TerrainParameters {
        let (z_g, alpha_r, z_min) = match self {
            TerrainCategory::II => (300.0, 0.16, 5.0),
            TerrainCategory::IIa => (380.0, 0.19, 5.0),
            TerrainCategory::III => (450.0, 0.23, 5.0),
            TerrainCategory::IV => (526.0, 0.30, 10.0),
        };
        TerrainParameters {
            z_g,
            alpha_r,
            z_min,
        }
    }
}

/// Snow exposure of the site (SIA 261 §5.2.4), giving the exposure coefficient C_e.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnowExposure {
    /// Exposed to wind on all sides; snow is blown off.
    Windswept,
    Normal,
    /// Sheltered by terrain, higher buildings or trees (K2: "Paysage protégé contre les
    /// intempéries").
    Sheltered,
}

impl SnowExposure {
    #[must_use]
    pub fn coefficient(self) -> f64 {
        match self {
            SnowExposure::Windswept => 0.8,
            SnowExposure::Normal => 1.0,
            SnowExposure::Sheltered => 1.2,
        }
    }
}

/// Consequence class (SIA 260 / EN 1990 annex B), giving the importance factor K_FI that
/// multiplies the actions in the combinations (K2: "Fl,G", "Fl,Q").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsequenceClass {
    CC1,
    CC2,
    CC3,
}

impl ConsequenceClass {
    #[must_use]
    pub fn importance_factor(self) -> f64 {
        match self {
            ConsequenceClass::CC1 => 0.9,
            ConsequenceClass::CC2 => 1.0,
            ConsequenceClass::CC3 => 1.1,
        }
    }
}

/// How the service-life adjustment of snow and wind is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ServiceLifeModel {
    /// EN 1991-1-3 annex D (snow) and EN 1991-1-4 §4.2 note 4 (wind), with the annual
    /// exceedance probability that gives, over the service life, the same reliability as a
    /// 50-year return period over 50 years. Gives 1.0 for 50 years.
    #[default]
    En1991,
    /// Explicit factors, e.g. from a manufacturer report or an engineer's decision.
    Explicit { snow: f64, wind: f64 },
}

/// Site and building data for one statics case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteInput {
    /// Terrain altitude above sea level [m].
    pub altitude_m: f64,
    /// Correction from the snow zone map (SIA 261 annexe D) added to the altitude to get the
    /// reference altitude h₀ [m]. K2: "Correction de l'altitude".
    #[serde(default)]
    pub snow_altitude_correction_m: f64,
    /// Reference value of the peak velocity pressure q_p0 (SIA 261 annexe E) [kN/m²].
    pub wind_reference_pressure_kn_m2: f64,
    pub terrain: TerrainCategory,
    /// Building height used as reference height z for the wind profile [m].
    pub building_height_m: f64,
    pub snow_exposure: SnowExposure,
    /// Thermal coefficient C_t [-]; 1.0 unless the roof is heated and poorly insulated.
    #[serde(default = "one")]
    pub thermal_coefficient: f64,
    /// Design service life [years].
    #[serde(default = "fifty")]
    pub service_life_years: f64,
    #[serde(default = "default_cc")]
    pub consequence_class: ConsequenceClass,
    #[serde(default)]
    pub service_life_model: ServiceLifeModel,
}

fn one() -> f64 {
    1.0
}
fn fifty() -> f64 {
    50.0
}
fn default_cc() -> ConsequenceClass {
    ConsequenceClass::CC2
}

impl SiteInput {
    /// Check that the inputs are physically meaningful and within the validity of the
    /// formulas used.
    pub fn validate(&self) -> Result<(), StaticsError> {
        StaticsError::check_range("altitude_m", self.altitude_m, -500.0, 5000.0)?;
        StaticsError::check_range(
            "reference_altitude_m",
            self.reference_altitude().value,
            -500.0,
            5000.0,
        )?;
        StaticsError::check_range(
            "wind_reference_pressure_kn_m2",
            self.wind_reference_pressure_kn_m2,
            0.1,
            10.0,
        )?;
        StaticsError::check_range("building_height_m", self.building_height_m, 0.5, 200.0)?;
        StaticsError::check_range("thermal_coefficient", self.thermal_coefficient, 0.5, 1.5)?;
        StaticsError::check_range("service_life_years", self.service_life_years, 1.0, 200.0)?;
        if let ServiceLifeModel::Explicit { snow, wind } = self.service_life_model {
            StaticsError::check_range("service_life_factor_snow", snow, 0.1, 2.0)?;
            StaticsError::check_range("service_life_factor_wind", wind, 0.1, 2.0)?;
        }
        Ok(())
    }

    /// Reference altitude h₀ = altitude + correction from the snow zone map.
    #[must_use]
    pub fn reference_altitude(&self) -> Traced {
        let h0 = self.altitude_m + self.snow_altitude_correction_m;
        Traced::new("h₀", h0, Unit::Metre)
            .formula("h₀ = h + Δh")
            .reference(SIA_261_ANNEX_D)
            .input("h", self.altitude_m, Unit::Metre)
            .input("Δh", self.snow_altitude_correction_m, Unit::Metre)
    }
}
