//! Dead load and load combinations (SIA 260).
//!
//! Actions are split into components perpendicular (⊥) and parallel (∥) to the roof plane.
//! Dead load and snow act vertically: ⊥ = q·cos α, ∥ = q·sin α. Wind acts perpendicular only.
//! Sign convention for ⊥: positive towards the roof (pressure), negative away (uplift).

use serde::{Deserialize, Serialize};

use crate::trace::{NormRef, Traced, Unit};
use crate::StaticsError;

pub const SIA_260_COMBINATIONS: NormRef = NormRef::new("SIA 260:2013", "§4.4.3");
/// Standard gravity used to convert masses to loads [m/s²].
pub const GRAVITY: f64 = 9.81;

/// Self weight of module and mounting system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeadLoad {
    pub module_mass_kg: f64,
    /// Mounting system mass per module (rails, clamps, hooks), without ballast [kg].
    pub system_mass_per_module_kg: f64,
    pub module_area_m2: f64,
}

/// Dead-load results, per m² of module surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeadLoadResult {
    pub module_mass_per_m2: Traced,
    pub system_mass_per_m2: Traced,
    /// Characteristic dead load g_k, vertical [kN/m²].
    pub load: Traced,
}

impl DeadLoad {
    pub fn evaluate(&self) -> Result<DeadLoadResult, StaticsError> {
        StaticsError::check_range("module_area_m2", self.module_area_m2, 0.1, 10.0)?;
        StaticsError::check_range("module_mass_kg", self.module_mass_kg, 0.0, 200.0)?;
        StaticsError::check_range(
            "system_mass_per_module_kg",
            self.system_mass_per_module_kg,
            0.0,
            500.0,
        )?;
        let a = self.module_area_m2;
        let module_mass_per_m2 =
            Traced::new("m_M", self.module_mass_kg / a, Unit::KilogramPerSquareMetre)
                .formula("m_M = G_M / A_M")
                .input("G_M", self.module_mass_kg, Unit::Kilogram)
                .input("A_M", a, Unit::SquareMetre);
        let system_mass_per_m2 = Traced::new(
            "m_S",
            self.system_mass_per_module_kg / a,
            Unit::KilogramPerSquareMetre,
        )
        .formula("m_S = G_S / A_M")
        .input("G_S", self.system_mass_per_module_kg, Unit::Kilogram)
        .input("A_M", a, Unit::SquareMetre);
        let load = Traced::new(
            "g_k",
            (module_mass_per_m2.value + system_mass_per_m2.value) * GRAVITY / 1000.0,
            Unit::KiloNewtonPerSquareMetre,
        )
        .formula("g_k = (m_M + m_S)·g")
        .input_from(&module_mass_per_m2)
        .input_from(&system_mass_per_m2)
        .input("g", GRAVITY, Unit::Dimensionless)
        .assume(
            "statics.assumption.dead_load.no_ballast",
            "Ballast not included",
        );
        Ok(DeadLoadResult {
            module_mass_per_m2,
            system_mass_per_m2,
            load,
        })
    }
}

/// Partial and combination factors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PartialFactors {
    /// Permanent action, unfavourable (STR).
    pub gamma_g_sup: f64,
    /// Permanent action, favourable (STR).
    pub gamma_g_inf: f64,
    /// Permanent action, destabilising (EQU).
    pub gamma_g_dst: f64,
    /// Permanent action, stabilising (EQU).
    pub gamma_g_stb: f64,
    /// Variable action.
    pub gamma_q: f64,
    /// Combination factor ψ₀ for wind as accompanying action.
    pub psi0_wind: f64,
    /// Combination factor ψ₀ for snow as accompanying action.
    pub psi0_snow: f64,
}

impl PartialFactors {
    /// Factor set of the "SIA / SN EN" design option, as printed by K2 Base 3.2.109
    /// (γ_G,sup 1.35, γ_G,inf 0.90, γ_G,dst 1.10, γ_G,stb 0.90, γ_Q 1.50, ψ₀,W 0.60, ψ₀,S 0.85).
    #[must_use]
    pub const fn sia_260() -> Self {
        Self {
            gamma_g_sup: 1.35,
            gamma_g_inf: 0.90,
            gamma_g_dst: 1.10,
            gamma_g_stb: 0.90,
            gamma_q: 1.50,
            psi0_wind: 0.60,
            psi0_snow: 0.85,
        }
    }
}

impl Default for PartialFactors {
    fn default() -> Self {
        Self::sia_260()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimitState {
    /// Ultimate limit state, structural resistance (STR).
    Uls,
    /// Serviceability limit state, characteristic combination.
    Sls,
}

/// Combinations, numbered as in K2 Base reports (CC 05 is not used there either).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombinationId {
    /// Snow leading.
    Cc01,
    /// Wind pressure leading, no snow.
    Cc02,
    /// Wind pressure leading, snow accompanying.
    Cc03,
    /// Snow leading, wind pressure accompanying.
    Cc04,
    /// Wind suction (uplift), permanent action favourable.
    Cc06,
}

impl CombinationId {
    pub const ALL: [CombinationId; 5] = [
        CombinationId::Cc01,
        CombinationId::Cc02,
        CombinationId::Cc03,
        CombinationId::Cc04,
        CombinationId::Cc06,
    ];

    #[must_use]
    pub fn formula(self, state: LimitState) -> &'static str {
        match (self, state) {
            (CombinationId::Cc01, LimitState::Uls) => "γ_G,sup·K_FI·G_k + γ_Q·K_FI·S_k",
            (CombinationId::Cc02, LimitState::Uls) => "γ_G,sup·K_FI·G_k + γ_Q·K_FI·W_k,pressure",
            (CombinationId::Cc03, LimitState::Uls) => {
                "γ_G,sup·K_FI·G_k + γ_Q·K_FI·(W_k,pressure + ψ₀,S·S_k)"
            }
            (CombinationId::Cc04, LimitState::Uls) => {
                "γ_G,sup·K_FI·G_k + γ_Q·K_FI·(S_k + ψ₀,W·W_k,pressure)"
            }
            (CombinationId::Cc06, LimitState::Uls) => "γ_G,inf·G_k + γ_Q·K_FI·W_k,suction",
            (CombinationId::Cc01, LimitState::Sls) => "G_k + S_k",
            (CombinationId::Cc02, LimitState::Sls) => "G_k + W_k,pressure",
            (CombinationId::Cc03, LimitState::Sls) => "G_k + W_k,pressure + ψ₀,S·S_k",
            (CombinationId::Cc04, LimitState::Sls) => "G_k + S_k + ψ₀,W·W_k,pressure",
            (CombinationId::Cc06, LimitState::Sls) => "G_k + W_k,suction",
        }
    }
}

/// Characteristic actions on one roof zone, per m² of roof/module surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SurfaceActions {
    pub pitch_deg: f64,
    /// Dead load g_k, vertical [kN/m²].
    pub dead: f64,
    /// Snow s_k per m² of roof surface, vertical [kN/m²].
    pub snow: f64,
    /// Wind pressure, perpendicular, ≥ 0 [kN/m²].
    pub wind_pressure: f64,
    /// Wind suction, perpendicular, ≤ 0 [kN/m²].
    pub wind_suction: f64,
}

/// Value of one combination.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CombinationValue {
    pub id: CombinationId,
    pub perpendicular: f64,
    pub parallel: f64,
}

/// Design envelope of one zone for one limit state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DesignEnvelope {
    pub limit_state: LimitState,
    /// Max ⊥ load towards the roof over CC01–CC04 [kN/m²].
    pub pressure_perp: Traced,
    /// Max ∥ load over CC01–CC04 [kN/m²].
    pub pressure_par: Traced,
    /// ⊥ load of the uplift combination CC06 (negative = uplift) [kN/m²].
    pub uplift_perp: Traced,
    /// ∥ load of the uplift combination CC06 [kN/m²].
    pub uplift_par: Traced,
    pub combinations: Vec<CombinationValue>,
}

/// Evaluate all combinations of one limit state.
#[must_use]
pub fn combine(
    actions: &SurfaceActions,
    factors: &PartialFactors,
    importance: f64,
    state: LimitState,
) -> DesignEnvelope {
    let (sin, cos) = actions.pitch_deg.to_radians().sin_cos();
    let (g_perp, g_par) = (actions.dead * cos, actions.dead * sin);
    let (s_perp, s_par) = (actions.snow * cos, actions.snow * sin);
    let (wp, ws) = (actions.wind_pressure, actions.wind_suction);

    let (g_sup, g_inf, q) = match state {
        LimitState::Uls => (
            factors.gamma_g_sup * importance,
            factors.gamma_g_inf,
            factors.gamma_q * importance,
        ),
        LimitState::Sls => (1.0, 1.0, 1.0),
    };
    let (psi_w, psi_s) = (factors.psi0_wind, factors.psi0_snow);

    let combinations: Vec<CombinationValue> = CombinationId::ALL
        .iter()
        .map(|&id| {
            let (perpendicular, parallel) = match id {
                CombinationId::Cc01 => (g_sup * g_perp + q * s_perp, g_sup * g_par + q * s_par),
                CombinationId::Cc02 => (g_sup * g_perp + q * wp, g_sup * g_par),
                CombinationId::Cc03 => (
                    g_sup * g_perp + q * (wp + psi_s * s_perp),
                    g_sup * g_par + q * psi_s * s_par,
                ),
                CombinationId::Cc04 => (
                    g_sup * g_perp + q * (s_perp + psi_w * wp),
                    g_sup * g_par + q * s_par,
                ),
                CombinationId::Cc06 => (g_inf * g_perp + q * ws, g_inf * g_par),
            };
            CombinationValue {
                id,
                perpendicular,
                parallel,
            }
        })
        .collect();

    let governing = |key: fn(&CombinationValue) -> f64| {
        combinations
            .iter()
            .filter(|c| c.id != CombinationId::Cc06)
            .copied()
            .max_by(|a, b| key(a).total_cmp(&key(b)))
            .expect("pressure combinations are not empty")
    };
    let cc06 = combinations[4];
    let suffix = match state {
        LimitState::Uls => "d",
        LimitState::Sls => "sls",
    };
    let traced = |symbol: &str, value: f64, id: CombinationId| {
        Traced::new(
            format!("{symbol}_{suffix}"),
            value,
            Unit::KiloNewtonPerSquareMetre,
        )
        .formula(format!("{:?}: {}", id, id.formula(state)))
        .reference(SIA_260_COMBINATIONS)
        .input("G_k,⊥", g_perp, Unit::KiloNewtonPerSquareMetre)
        .input("G_k,∥", g_par, Unit::KiloNewtonPerSquareMetre)
        .input("S_k,⊥", s_perp, Unit::KiloNewtonPerSquareMetre)
        .input("S_k,∥", s_par, Unit::KiloNewtonPerSquareMetre)
        .input("W_k,pressure", wp, Unit::KiloNewtonPerSquareMetre)
        .input("W_k,suction", ws, Unit::KiloNewtonPerSquareMetre)
    };
    let p_perp = governing(|c| c.perpendicular);
    let p_par = governing(|c| c.parallel);
    DesignEnvelope {
        limit_state: state,
        pressure_perp: traced("q⊥,pressure", p_perp.perpendicular, p_perp.id),
        pressure_par: traced("q∥,pressure", p_par.parallel, p_par.id),
        uplift_perp: traced("q⊥,uplift", cc06.perpendicular, CombinationId::Cc06),
        uplift_par: traced("q∥,uplift", cc06.parallel, CombinationId::Cc06),
        combinations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_roof_has_no_parallel_component() {
        let a = SurfaceActions {
            pitch_deg: 0.0,
            dead: 0.12,
            snow: 0.72,
            wind_pressure: 0.1,
            wind_suction: -0.8,
        };
        let env = combine(&a, &PartialFactors::sia_260(), 1.0, LimitState::Uls);
        assert!(env.pressure_par.value.abs() < 1e-12);
        // CC06: 0.9·0.12 − 1.5·0.8
        assert!((env.uplift_perp.value - (0.108 - 1.2)).abs() < 1e-12);
        // CC04 governs: 1.35·0.12 + 1.5·(0.72 + 0.6·0.1) = 1.332 > CC01 1.242 > CC03 1.230
        assert!((env.pressure_perp.value - (0.162 + 1.5 * 0.78)).abs() < 1e-12);
        assert!(env.pressure_perp.formula.starts_with("Cc04"));
    }

    #[test]
    fn sls_uses_unit_factors() {
        let a = SurfaceActions {
            pitch_deg: 0.0,
            dead: 0.1,
            snow: 0.5,
            wind_pressure: 0.2,
            wind_suction: -0.6,
        };
        let env = combine(&a, &PartialFactors::sia_260(), 1.0, LimitState::Sls);
        assert!((env.uplift_perp.value - (0.1 - 0.6)).abs() < 1e-12);
        // CC03 governs: 0.1 + 0.2 + 0.85·0.5 = 0.725 > CC04 0.1 + 0.5 + 0.6·0.2 = 0.720
        assert!((env.pressure_perp.value - (0.1 + 0.2 + 0.85 * 0.5)).abs() < 1e-12);
        assert!(env.pressure_perp.formula.starts_with("Cc03"));
    }
}
