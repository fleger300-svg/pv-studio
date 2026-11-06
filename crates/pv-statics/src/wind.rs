//! Wind: peak velocity pressure, SIA 261 chapter 6.
//!
//! q_p = c_h·q_p0, with the height profile c_h = 1.6·[(z/z_g)^α_r + 0.375]², then adjusted to
//! the service life. Pressure coefficients per roof zone are in [`crate::cp`].

use serde::Serialize;

use crate::site::{ServiceLifeModel, SiteInput, SIA_261_TERRAIN};
use crate::trace::{NormRef, Traced, Unit};
use crate::{lifetime_annual_probability, StaticsError};

pub const SIA_261_PROFILE: NormRef = NormRef::new("SIA 261:2014", "§6.2.1.2, (eq. 41)");
pub const SIA_261_PEAK_PRESSURE: NormRef = NormRef::new("SIA 261:2014", "§6.2.1.1, (eq. 40)");
pub const EN_1991_1_4_PROBABILITY: NormRef =
    NormRef::new("SN EN 1991-1-4", "§4.2 (2)P, note 4, (4.2)");

/// Shape parameter K of the extreme-value distribution (EN 1991-1-4, recommended value).
pub const WIND_PROBABILITY_K: f64 = 0.2;
/// Exponent n (EN 1991-1-4, recommended value). It applies to the velocity; the pressure
/// factor is its square.
pub const WIND_PROBABILITY_N: f64 = 0.5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindResult {
    pub height_coefficient: Traced,
    /// Peak velocity pressure, 50-year return period [kN/m²].
    pub peak_pressure_50: Traced,
    pub service_life_factor: Traced,
    /// Peak velocity pressure for the design service life [kN/m²].
    pub peak_pressure: Traced,
}

/// Height profile coefficient c_h(z) for a terrain category.
pub fn height_coefficient(site: &SiteInput) -> Result<Traced, StaticsError> {
    site.validate()?;
    let p = site.terrain.parameters();
    let z = site.building_height_m;
    let z_eff = z.max(p.z_min);
    let value = 1.6 * ((z_eff / p.z_g).powf(p.alpha_r) + 0.375).powi(2);
    let mut t = Traced::new("c_h", value, Unit::Dimensionless)
        .formula("c_h = 1.6·[(z/z_g)^α_r + 0.375]², z ≥ z_min")
        .reference(SIA_261_PROFILE)
        .input("z", z, Unit::Metre)
        .input("z_g", p.z_g, Unit::Metre)
        .input("α_r", p.alpha_r, Unit::Dimensionless)
        .input("z_min", p.z_min, Unit::Metre)
        .assume(
            "statics.assumption.wind.terrain",
            format!(
                "Terrain category {:?} ({} {})",
                site.terrain, SIA_261_TERRAIN.document, SIA_261_TERRAIN.clause
            ),
        )
        .assume(
            "statics.assumption.wind.reference_height",
            "Reference height z = building height",
        );
    if z < p.z_min {
        t = t.assume(
            "statics.assumption.wind.below_z_min",
            format!("z = {z:.2} m < z_min = {:.1} m: z_min is used", p.z_min),
        );
    }
    Ok(t)
}

/// Service-life adjustment of the wind pressure, EN 1991-1-4 (4.2):
/// f_w = c_prob² = [(1 − K·ln(−ln(1 − p))) / (1 − K·ln(−ln 0.98))]^(2n).
#[must_use]
pub fn service_life_factor(site: &SiteInput) -> Traced {
    let years = site.service_life_years;
    match site.service_life_model {
        ServiceLifeModel::Explicit { wind, .. } => Traced::given("f_w", wind, Unit::Dimensionless)
            .input("t_L", years, Unit::Year)
            .assume(
                "statics.assumption.service_life.explicit",
                "Service-life factor given explicitly",
            ),
        ServiceLifeModel::En1991 => {
            let p = lifetime_annual_probability(years);
            let (k, n) = (WIND_PROBABILITY_K, WIND_PROBABILITY_N);
            let num = 1.0 - k * (-(1.0 - p).ln()).ln();
            let den = 1.0 - k * (-(0.98_f64).ln()).ln();
            let value = (num / den).powf(2.0 * n);
            Traced::new("f_w", value, Unit::Dimensionless)
                .formula("f_w = c_prob² = [(1 − K·ln(−ln(1 − p))) / (1 − K·ln(−ln 0.98))]^(2n), p = 1 − 0.98^(50/t_L)")
                .reference(EN_1991_1_4_PROBABILITY)
                .input("t_L", years, Unit::Year)
                .input("p", p, Unit::Dimensionless)
                .input("K", k, Unit::Dimensionless)
                .input("n", n, Unit::Dimensionless)
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

/// Full wind-pressure chain for the site.
pub fn peak_pressure(site: &SiteInput) -> Result<WindResult, StaticsError> {
    let height_coefficient = height_coefficient(site)?;
    let qp0 = Traced::given(
        "q_p0",
        site.wind_reference_pressure_kn_m2,
        Unit::KiloNewtonPerSquareMetre,
    )
    .reference(NormRef::new("SIA 261:2014", "Annexe E"));
    let peak_pressure_50 = Traced::new(
        "q_p,50",
        height_coefficient.value * qp0.value,
        Unit::KiloNewtonPerSquareMetre,
    )
    .formula("q_p = c_h·q_p0")
    .reference(SIA_261_PEAK_PRESSURE)
    .input_from(&height_coefficient)
    .input_from(&qp0);
    let service_life_factor = service_life_factor(site);
    let peak_pressure = Traced::new(
        "q_p",
        peak_pressure_50.value * service_life_factor.value,
        Unit::KiloNewtonPerSquareMetre,
    )
    .formula("q_p = f_w·q_p,50")
    .input_from(&service_life_factor)
    .input_from(&peak_pressure_50);
    Ok(WindResult {
        height_coefficient,
        peak_pressure_50,
        service_life_factor,
        peak_pressure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::TerrainCategory;

    #[test]
    fn profile_grows_with_height_and_drops_with_roughness() {
        let mut site = crate::tests::site_fixture(50.0);
        site.terrain = TerrainCategory::II;
        site.building_height_m = 10.0;
        let ch_ii_10 = height_coefficient(&site).unwrap().value;
        // 1.6·[(10/300)^0.16 + 0.375]² = 1.460
        assert!((ch_ii_10 - 1.460).abs() < 1e-3, "c_h = {ch_ii_10}");
        site.building_height_m = 20.0;
        assert!(height_coefficient(&site).unwrap().value > ch_ii_10);
        site.building_height_m = 10.0;
        site.terrain = TerrainCategory::IV;
        assert!(height_coefficient(&site).unwrap().value < ch_ii_10);
    }

    #[test]
    fn below_z_min_is_clamped() {
        let mut site = crate::tests::site_fixture(50.0);
        site.terrain = TerrainCategory::IV;
        site.building_height_m = 4.0;
        let low = height_coefficient(&site).unwrap();
        site.building_height_m = 10.0;
        let at_min = height_coefficient(&site).unwrap();
        assert_eq!(low.value, at_min.value);
        assert!(low
            .assumptions
            .iter()
            .any(|a| a.key == "statics.assumption.wind.below_z_min"));
    }

    #[test]
    fn service_life_50_years_is_neutral() {
        let site = crate::tests::site_fixture(50.0);
        assert!((service_life_factor(&site).value - 1.0).abs() < 1e-12);
    }
}
