//! Snow load, SIA 261 chapter 5.
//!
//! Chain: reference altitude h₀ → ground snow load s_k → roof snow load
//! s = μ·C_e·C_t·s_k, expressed per m² of roof surface (× cos α), then adjusted to the
//! service life.

use serde::Serialize;

use crate::site::{ServiceLifeModel, SiteInput};
use crate::trace::{NormRef, Traced, Unit};
use crate::{lifetime_annual_probability, StaticsError};

pub const SIA_261_GROUND_SNOW: NormRef = NormRef::new("SIA 261:2014", "§5.2.2, (eq. 1)");
pub const SIA_261_SHAPE: NormRef = NormRef::new("SIA 261:2014", "§5.3.2");
pub const SIA_261_ROOF_SNOW: NormRef = NormRef::new("SIA 261:2014", "§5.2.1");
pub const EN_1991_1_3_ANNEX_D: NormRef = NormRef::new("SN EN 1991-1-3", "Annex D, (D.1)");

/// Minimum ground snow load [kN/m²].
pub const MIN_GROUND_SNOW_KN_M2: f64 = 0.9;
/// Coefficient of variation of the annual snow maximum used for the service-life adjustment
/// (EN 1991-1-3 annex D, recommended value).
pub const SNOW_COEFFICIENT_OF_VARIATION: f64 = 0.2;

/// Snow results for one roof face.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnowResult {
    pub reference_altitude: Traced,
    pub ground_load: Traced,
    pub shape_coefficient: Traced,
    pub exposure_coefficient: Traced,
    pub thermal_coefficient: Traced,
    /// cos α: converts the load on the horizontal projection into a load per m² of roof.
    pub slope_factor: Traced,
    /// Roof snow load, 50-year return period, per m² of roof surface [kN/m²].
    pub roof_load_50: Traced,
    pub service_life_factor: Traced,
    /// Roof snow load for the design service life, per m² of roof surface [kN/m²]. This is the
    /// characteristic value used in the combinations.
    pub roof_load: Traced,
}

/// Ground snow load s_k = 0.4·[1 + (h₀/350)²] ≥ 0.9 kN/m².
#[must_use]
pub fn ground_snow_load(reference_altitude: &Traced) -> Traced {
    let h0 = reference_altitude.value;
    let raw = 0.4 * (1.0 + (h0 / 350.0).powi(2));
    let value = raw.max(MIN_GROUND_SNOW_KN_M2);
    let mut t = Traced::new("s_k", value, Unit::KiloNewtonPerSquareMetre)
        .formula("s_k = max(0.4·[1 + (h₀/350)²], 0.9) kN/m²")
        .reference(SIA_261_GROUND_SNOW)
        .input_from(reference_altitude);
    if raw < MIN_GROUND_SNOW_KN_M2 {
        t = t.assume(
            "statics.assumption.snow.minimum_ground_load",
            format!("0.4·[1 + (h₀/350)²] = {raw:.3} kN/m² < 0.9 kN/m²: minimum value applies"),
        );
    }
    t
}

/// Shape coefficient μ₁ of a mono- or duopitch roof without snow guards, by pitch α [°]:
/// 0.8 up to 30°, linear to 0 at 60°, 0 above.
pub fn shape_coefficient(pitch_deg: f64) -> Result<Traced, StaticsError> {
    StaticsError::check_range("pitch_deg", pitch_deg, 0.0, 90.0)?;
    let value = if pitch_deg <= 30.0 {
        0.8
    } else if pitch_deg < 60.0 {
        0.8 * (60.0 - pitch_deg) / 30.0
    } else {
        0.0
    };
    Ok(Traced::new("μ₁", value, Unit::Dimensionless)
        .formula("μ₁ = 0.8 (α ≤ 30°); 0.8·(60° − α)/30° (30° < α < 60°); 0 (α ≥ 60°)")
        .reference(SIA_261_SHAPE)
        .input("α", pitch_deg, Unit::Degree)
        .assume(
            "statics.assumption.snow.no_drift",
            "Uniform load case without drift, accumulation or snow guards",
        ))
}

/// Service-life adjustment of the snow load, EN 1991-1-3 annex D:
/// f_s = [1 − V·√6/π·(ln(−ln(1 − p)) + 0.57722)] / (1 + 2.5923·V).
#[must_use]
pub fn service_life_factor(site: &SiteInput) -> Traced {
    let years = site.service_life_years;
    match site.service_life_model {
        ServiceLifeModel::Explicit { snow, .. } => Traced::given("f_s", snow, Unit::Dimensionless)
            .input("t_L", years, Unit::Year)
            .assume(
                "statics.assumption.service_life.explicit",
                "Service-life factor given explicitly",
            ),
        ServiceLifeModel::En1991 => {
            let p = lifetime_annual_probability(years);
            let v = SNOW_COEFFICIENT_OF_VARIATION;
            // 0.57722 in the standard is the Euler–Mascheroni constant.
            let gumbel = (-(1.0 - p).ln()).ln() + std::f64::consts::EULER_GAMMA;
            let value =
                (1.0 - v * 6.0_f64.sqrt() / std::f64::consts::PI * gumbel) / (1.0 + 2.5923 * v);
            Traced::new("f_s", value, Unit::Dimensionless)
                .formula("f_s = [1 − V·√6/π·(ln(−ln(1 − p)) + 0.57722)] / (1 + 2.5923·V), p = 1 − 0.98^(50/t_L)")
                .reference(EN_1991_1_3_ANNEX_D)
                .input("t_L", years, Unit::Year)
                .input("p", p, Unit::Dimensionless)
                .input("V", v, Unit::Dimensionless)
                .assume(
                    "statics.assumption.service_life.equal_reliability",
                    format!(
                        "Annual exceedance probability chosen so that the {years:.0}-year service life has the same \
                         reliability as a 50-year return period over 50 years"
                    ),
                )
        }
    }
}

/// Full snow chain for a roof face of pitch α.
pub fn roof_snow(site: &SiteInput, pitch_deg: f64) -> Result<SnowResult, StaticsError> {
    site.validate()?;
    let reference_altitude = site.reference_altitude();
    let ground_load = ground_snow_load(&reference_altitude);
    let shape_coefficient = shape_coefficient(pitch_deg)?;
    let exposure_coefficient =
        Traced::new("C_e", site.snow_exposure.coefficient(), Unit::Dimensionless)
            .formula("C_e from site exposure")
            .reference(NormRef::new("SIA 261:2014", "§5.2.4"))
            .assume(
                "statics.assumption.snow.exposure",
                format!("Site exposure: {:?}", site.snow_exposure),
            );
    let thermal_coefficient = Traced::given("C_t", site.thermal_coefficient, Unit::Dimensionless)
        .reference(NormRef::new("SIA 261:2014", "§5.2.5"));
    let cos = pitch_deg.to_radians().cos();
    let slope_factor = Traced::new("cos α", cos, Unit::Dimensionless)
        .formula("cos α")
        .input("α", pitch_deg, Unit::Degree)
        .assume(
            "statics.assumption.snow.per_roof_surface",
            "Snow load converted from horizontal projection to roof surface",
        );

    let s50 = shape_coefficient.value
        * exposure_coefficient.value
        * thermal_coefficient.value
        * ground_load.value
        * cos;
    let roof_load_50 = Traced::new("s_50", s50, Unit::KiloNewtonPerSquareMetre)
        .formula("s_50 = μ₁·C_e·C_t·s_k·cos α")
        .reference(SIA_261_ROOF_SNOW)
        .input_from(&shape_coefficient)
        .input_from(&exposure_coefficient)
        .input_from(&thermal_coefficient)
        .input_from(&ground_load)
        .input_from(&slope_factor);

    let service_life_factor = service_life_factor(site);
    let roof_load = Traced::new(
        "s_n",
        s50 * service_life_factor.value,
        Unit::KiloNewtonPerSquareMetre,
    )
    .formula("s_n = f_s·s_50")
    .input_from(&service_life_factor)
    .input_from(&roof_load_50);

    Ok(SnowResult {
        reference_altitude,
        ground_load,
        shape_coefficient,
        exposure_coefficient,
        thermal_coefficient,
        slope_factor,
        roof_load_50,
        service_life_factor,
        roof_load,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h0(v: f64) -> Traced {
        Traced::given("h₀", v, Unit::Metre)
    }

    #[test]
    fn ground_load_follows_altitude_and_minimum() {
        assert!((ground_snow_load(&h0(200.0)).value - 0.9).abs() < 1e-12);
        // 0.4·(1 + (700/350)²) = 2.0
        assert!((ground_snow_load(&h0(700.0)).value - 2.0).abs() < 1e-12);
        assert!(ground_snow_load(&h0(200.0)).assumptions.len() == 1);
    }

    #[test]
    fn shape_coefficient_by_pitch() {
        assert_eq!(shape_coefficient(0.0).unwrap().value, 0.8);
        assert_eq!(shape_coefficient(30.0).unwrap().value, 0.8);
        assert!((shape_coefficient(45.0).unwrap().value - 0.4).abs() < 1e-12);
        assert_eq!(shape_coefficient(60.0).unwrap().value, 0.0);
        assert!(shape_coefficient(-1.0).is_err());
    }

    #[test]
    fn service_life_50_years_is_neutral() {
        let site = crate::tests::site_fixture(50.0);
        assert!((service_life_factor(&site).value - 1.0).abs() < 1e-4);
    }
}
