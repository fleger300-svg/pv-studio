//! Load descent: surface load → rails → fixations, rail checks and maximum spacings.
//!
//! The model is the two-layer cross-rail system (K2 SingleRail with climbers): modules sit on
//! top rails, top rails sit on bottom rails, bottom rails are fixed to the roof. A single-layer
//! system is the same chain with only the bottom layer.
//!
//! - Top rail: line load q = p·w (w = tributary width of the rail), continuous beam over the
//!   bottom rails: M = q·L²/10, δ = q·L⁴/(185·E·I).
//! - Bottom rail: line load q = κ·p·s_top (κ = support-reaction factor of the continuous top
//!   rail, s_top = spacing of the bottom rails), simple span between fixations: M = q·L²/8,
//!   δ = 5·q·L⁴/(384·E·I).
//! - Fixation force: F = κ·p·s_top·s_fix.
//!
//! κ = 1.05, the bending coefficients and the deflection limit of the K2 SingleRail 36 are
//! calibrated on K2 Base reports (references `ref-a` and `ref-c-roof1`); they are recorded as
//! assumptions on every result.

use serde::{Deserialize, Serialize};

use crate::trace::{NormRef, Traced, Unit};
use crate::StaticsError;

pub const EN_1999_BENDING: NormRef = NormRef::new("SN EN 1999-1-1", "§6.2.5");
pub const EN_1999_SLS: NormRef = NormRef::new("SN EN 1999-1-1", "§7.2");

/// Reaction factor of a continuous top rail on its supports (bottom rails).
pub const SUPPORT_REACTION_FACTOR: f64 = 1.05;
/// Bending coefficient of a continuous rail (M = q·L²/10).
pub const CONTINUOUS_MOMENT_DIVISOR: f64 = 10.0;
/// Deflection coefficient of a continuous rail (two equal spans, δ_max = q·L⁴/(185·E·I)).
pub const CONTINUOUS_DEFLECTION_DIVISOR: f64 = 185.0;

/// Cross-section and material of a rail profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RailSection {
    pub name: String,
    pub area_cm2: f64,
    pub iy_cm4: f64,
    pub iz_cm4: f64,
    pub wy_cm3: f64,
    pub wz_cm3: f64,
    /// 0.2 % proof strength f₀ [MPa].
    pub f0_mpa: f64,
    pub gamma_m1: f64,
    pub e_mpa: f64,
    /// Admissible deflection under the characteristic combination [m].
    pub deflection_limit_m: f64,
}

impl RailSection {
    /// K2 SingleRail 36 (section values as printed in K2 Base; EN AW-6063 T66).
    #[must_use]
    pub fn k2_single_rail_36() -> Self {
        Self {
            name: "K2 SingleRail 36".into(),
            area_cm2: 2.850,
            iy_cm4: 4.02,
            iz_cm4: 6.37,
            wy_cm3: 2.14,
            wz_cm3: 3.09,
            f0_mpa: 200.0,
            gamma_m1: 1.1,
            e_mpa: 70_000.0,
            // Calibrated: q_k·L⁴ = 6.208 kN·m³ at 100 % on both references.
            deflection_limit_m: 0.011_925,
        }
    }

    /// Design bending resistance M_Rd = W_y·f₀/γ_M1 [kN·m].
    #[must_use]
    pub fn moment_resistance(&self) -> Traced {
        let value = self.wy_cm3 * 1e-6 * self.f0_mpa * 1e3 / self.gamma_m1;
        Traced::new("M_Rd", value, Unit::KiloNewton)
            .formula("M_Rd = W_y·f₀/γ_M1 [kN·m]")
            .reference(EN_1999_BENDING)
            .input("W_y", self.wy_cm3, Unit::Dimensionless)
            .input("f₀", self.f0_mpa, Unit::Dimensionless)
            .input("γ_M1", self.gamma_m1, Unit::Dimensionless)
    }

    /// Bending stiffness E·I [kN·m²].
    #[must_use]
    pub fn stiffness_kn_m2(&self) -> f64 {
        self.e_mpa * 1e3 * self.iy_cm4 * 1e-8
    }
}

/// Static scheme of a rail between its supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeamScheme {
    /// Continuous over several supports (top rail on bottom rails).
    Continuous,
    /// Simple span between supports (bottom rail between fixations).
    SimpleSpan,
}

impl BeamScheme {
    fn moment_divisor(self) -> f64 {
        match self {
            BeamScheme::Continuous => CONTINUOUS_MOMENT_DIVISOR,
            BeamScheme::SimpleSpan => 8.0,
        }
    }
    fn deflection_divisor(self) -> f64 {
        match self {
            BeamScheme::Continuous => CONTINUOUS_DEFLECTION_DIVISOR,
            BeamScheme::SimpleSpan => 384.0 / 5.0,
        }
    }
}

/// Grid on which the roof fixations can be placed; it decides the fixation spacing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SupportGrid {
    /// One clamp on every seam. K2 Base does not optimise standing-seam clamps: they go on
    /// every seam and their resistance is not checked ("seule la capacité des rails est
    /// éprouvée").
    StandingSeam { seam_spacing_m: f64 },
    /// Fixations on rafters / purlins: the spacing is the largest multiple of the rafter
    /// spacing that does not exceed the maximum spacing.
    Rafters { spacing_m: f64 },
}

/// Design resistance of one fixation [kN].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixationResistance {
    pub name: String,
    pub uplift_kn: f64,
    pub pressure_kn: f64,
    pub parallel_kn: f64,
}

/// Cross-rail mounting system of one field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossRailSystem {
    pub top_rail: RailSection,
    pub bottom_rail: RailSection,
    /// Width of roof surface carried by one top rail [m] (module dimension across the rail
    /// divided by the number of top rails per module).
    pub top_rail_tributary_width_m: f64,
    /// Spacing of the bottom rails = span of the top rail [m].
    pub bottom_rail_spacing_m: f64,
    pub support_grid: SupportGrid,
    /// Largest fixation spacing that actually occurs on the bottom rails [m]; defaults to the
    /// spacing chosen from the grid.
    #[serde(default)]
    pub actual_fixation_spacing_m: Option<f64>,
    #[serde(default)]
    pub fixation: Option<FixationResistance>,
}

/// Design surface loads of one zone, ⊥ / ∥ to the roof [kN/m²] (from [`crate::combos`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoneSurfaceLoads {
    pub uls_pressure_perp: f64,
    pub uls_pressure_par: f64,
    /// Negative = uplift.
    pub uls_uplift_perp: f64,
    pub uls_uplift_par: f64,
    pub sls_pressure_perp: f64,
    pub sls_pressure_par: f64,
    pub sls_uplift_perp: f64,
    pub sls_uplift_par: f64,
}

impl ZoneSurfaceLoads {
    #[must_use]
    pub fn from_envelopes(uls: &crate::DesignEnvelope, sls: &crate::DesignEnvelope) -> Self {
        Self {
            uls_pressure_perp: uls.pressure_perp.value,
            uls_pressure_par: uls.pressure_par.value,
            uls_uplift_perp: uls.uplift_perp.value,
            uls_uplift_par: uls.uplift_par.value,
            sls_pressure_perp: sls.pressure_perp.value,
            sls_pressure_par: sls.pressure_par.value,
            sls_uplift_perp: sls.uplift_perp.value,
            sls_uplift_par: sls.uplift_par.value,
        }
    }
}

/// Forces on one fixation [kN]; ⊥ negative = uplift.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixationForces {
    pub tributary_area: Traced,
    pub uls_pressure_perp: f64,
    pub uls_pressure_par: f64,
    pub uls_uplift_perp: f64,
    pub uls_uplift_par: f64,
    pub sls_pressure_perp: f64,
    pub sls_pressure_par: f64,
    pub sls_uplift_perp: f64,
    pub sls_uplift_par: f64,
}

/// Check of one rail layer at its actual span, and its maximum span.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RailCheck {
    pub span_m: f64,
    /// Bending utilisation (max of pressure and uplift), ULS.
    pub bending_utilisation: Traced,
    /// Deflection utilisation, SLS.
    pub deflection_utilisation: Traced,
    /// Maximum span (spacing of the supports of this rail) [m].
    pub max_span: Traced,
}

/// Fixation check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixationCheck {
    /// max(F⊥,pressure/R_pressure, F⊥,uplift/R_uplift).
    pub perpendicular_utilisation: Traced,
    /// F∥/R∥, reported separately (K2 Base does not include it in its "Fst F[%]").
    pub parallel_utilisation: Traced,
}

/// Load descent result of one zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DescentResult {
    pub top_rail: RailCheck,
    pub bottom_rail: RailCheck,
    /// Fixation spacing on the bottom rails, chosen from the support grid [m].
    pub fixation_spacing: Traced,
    pub fixation_forces: FixationForces,
    pub fixation_check: Option<FixationCheck>,
}

fn rail_check(
    section: &RailSection,
    scheme: BeamScheme,
    width_m: f64,
    span_m: f64,
    loads: &ZoneSurfaceLoads,
    deflection_limits_span: bool,
    extra_span_limit: Option<(f64, &'static str)>,
) -> RailCheck {
    let m_rd = section.moment_resistance().value;
    let ei = section.stiffness_kn_m2();
    let q_d = loads
        .uls_pressure_perp
        .abs()
        .max(loads.uls_uplift_perp.abs())
        * width_m;
    let q_k = loads
        .sls_pressure_perp
        .abs()
        .max(loads.sls_uplift_perp.abs())
        * width_m;
    let (cm, cd) = (scheme.moment_divisor(), scheme.deflection_divisor());

    let bending = q_d * span_m.powi(2) / cm / m_rd;
    let deflection = q_k * span_m.powi(4) / (cd * ei) / section.deflection_limit_m;
    let l_bending = (cm * m_rd / q_d).sqrt();
    let l_deflection = (cd * ei * section.deflection_limit_m / q_k).powf(0.25);

    let mut max_span = l_bending;
    let mut governing = "bending";
    if deflection_limits_span && l_deflection < max_span {
        max_span = l_deflection;
        governing = "deflection";
    }
    if let Some((l, name)) = extra_span_limit {
        if l < max_span {
            max_span = l;
            governing = name;
        }
    }
    let scheme_name = format!("{scheme:?}");
    RailCheck {
        span_m,
        bending_utilisation: Traced::new("η_M", bending, Unit::Dimensionless)
            .formula(format!("η_M = q_d·L²/{cm}/M_Rd"))
            .reference(EN_1999_BENDING)
            .input("q_d", q_d, Unit::KiloNewton)
            .input("L", span_m, Unit::Metre)
            .input("M_Rd", m_rd, Unit::KiloNewton)
            .assume(
                "statics.assumption.descent.scheme",
                format!("{} as {scheme_name} beam", section.name),
            ),
        deflection_utilisation: Traced::new("η_δ", deflection, Unit::Dimensionless)
            .formula(format!("η_δ = q_k·L⁴/({cd:.1}·E·I)/δ_lim"))
            .reference(EN_1999_SLS)
            .input("q_k", q_k, Unit::KiloNewton)
            .input("L", span_m, Unit::Metre)
            .input("E·I", ei, Unit::Dimensionless)
            .input("δ_lim", section.deflection_limit_m, Unit::Metre),
        max_span: Traced::new("L_max", max_span, Unit::Metre)
            .formula("L_max = min(√(c_M·M_Rd/q_d), ⁴√(c_δ·E·I·δ_lim/q_k), R_d/(F per m))")
            .input("L_M", l_bending, Unit::Metre)
            .input("L_δ", l_deflection, Unit::Metre)
            .assume(
                "statics.assumption.descent.governing",
                format!("Governed by {governing}"),
            ),
    }
}

/// Load descent and checks for one zone.
pub fn descend(
    system: &CrossRailSystem,
    loads: &ZoneSurfaceLoads,
) -> Result<DescentResult, StaticsError> {
    StaticsError::check_range(
        "top_rail_tributary_width_m",
        system.top_rail_tributary_width_m,
        0.05,
        5.0,
    )?;
    StaticsError::check_range(
        "bottom_rail_spacing_m",
        system.bottom_rail_spacing_m,
        0.05,
        5.0,
    )?;
    let kappa = SUPPORT_REACTION_FACTOR;
    let s_top = system.bottom_rail_spacing_m;

    // Top rail: continuous over the bottom rails; its maximum span is the maximum spacing of
    // the bottom rails.
    let top_rail = rail_check(
        &system.top_rail,
        BeamScheme::Continuous,
        system.top_rail_tributary_width_m,
        s_top,
        loads,
        true,
        None,
    );

    // Bottom rail fixation spacing.
    let bottom_width = kappa * s_top;
    let fixation_limit = system.fixation.as_ref().map(|r| {
        let per_m_pressure = kappa * loads.uls_pressure_perp.abs() * s_top;
        let per_m_uplift = kappa * loads.uls_uplift_perp.abs() * s_top;
        let l = (r.pressure_kn / per_m_pressure).min(r.uplift_kn / per_m_uplift);
        (l, "fixation resistance")
    });
    let free = rail_check(
        &system.bottom_rail,
        BeamScheme::SimpleSpan,
        bottom_width,
        1.0,
        loads,
        false,
        fixation_limit,
    );
    let (spacing, rule) = match system.support_grid {
        SupportGrid::StandingSeam { seam_spacing_m } => (
            seam_spacing_m,
            "Clamp on every seam (standing-seam clamps are not optimised)".to_string(),
        ),
        SupportGrid::Rafters { spacing_m } => {
            let n = (free.max_span.value / spacing_m + 1e-9).floor().max(1.0);
            (
                n * spacing_m,
                format!("Every {n:.0}. rafter (largest multiple of {spacing_m:.3} m ≤ L_max)"),
            )
        }
    };
    let fixation_spacing = Traced::new("s_fix", spacing, Unit::Metre)
        .formula("s_fix from support grid and L_max")
        .input_from(&free.max_span)
        .assume("statics.assumption.descent.fixation_grid", rule);
    let s_fix = system.actual_fixation_spacing_m.unwrap_or(spacing);
    let mut bottom_rail = rail_check(
        &system.bottom_rail,
        BeamScheme::SimpleSpan,
        bottom_width,
        s_fix,
        loads,
        false,
        fixation_limit,
    );
    if matches!(system.support_grid, SupportGrid::StandingSeam { .. }) {
        bottom_rail.max_span = bottom_rail.max_span.assume(
            "statics.assumption.descent.standing_seam",
            "Not governing: one clamp per seam",
        );
    }

    let area = kappa * s_top * s_fix;
    let tributary_area = Traced::new("A_fix", area, Unit::SquareMetre)
        .formula("A_fix = κ·s_top·s_fix")
        .input("κ", kappa, Unit::Dimensionless)
        .input("s_top", s_top, Unit::Metre)
        .input("s_fix", s_fix, Unit::Metre)
        .assume(
            "statics.assumption.descent.reaction_factor",
            "κ = 1.05: reaction of the continuous top rail, calibrated on K2 Base",
        );
    let fixation_forces = FixationForces {
        uls_pressure_perp: loads.uls_pressure_perp * area,
        uls_pressure_par: loads.uls_pressure_par * area,
        uls_uplift_perp: loads.uls_uplift_perp * area,
        uls_uplift_par: loads.uls_uplift_par * area,
        sls_pressure_perp: loads.sls_pressure_perp * area,
        sls_pressure_par: loads.sls_pressure_par * area,
        sls_uplift_perp: loads.sls_uplift_perp * area,
        sls_uplift_par: loads.sls_uplift_par * area,
        tributary_area,
    };
    let fixation_check = system.fixation.as_ref().map(|r| {
        let f = &fixation_forces;
        let perp =
            (f.uls_pressure_perp.abs() / r.pressure_kn).max(f.uls_uplift_perp.abs() / r.uplift_kn);
        let par = f.uls_pressure_par.abs().max(f.uls_uplift_par.abs()) / r.parallel_kn;
        FixationCheck {
            perpendicular_utilisation: Traced::new("η_F⊥", perp, Unit::Dimensionless)
                .formula("η = max(F⊥,pressure/R_pressure, F⊥,uplift/R_uplift)")
                .input("R_pressure", r.pressure_kn, Unit::KiloNewton)
                .input("R_uplift", r.uplift_kn, Unit::KiloNewton)
                .assume("statics.assumption.descent.fixation", r.name.clone()),
            parallel_utilisation: Traced::new("η_F∥", par, Unit::Dimensionless)
                .formula("η = F∥/R∥")
                .input("R∥", r.parallel_kn, Unit::KiloNewton),
        }
    });

    Ok(DescentResult {
        top_rail,
        bottom_rail,
        fixation_spacing,
        fixation_forces,
        fixation_check,
    })
}

// ---------------------------------------------------------------------------------------------
// Fixation counts
// ---------------------------------------------------------------------------------------------

/// Number of supports under a rail of length `rail_length_m`, centred on a support grid of
/// spacing `spacing_m`, keeping at least `min_overhang_m` at each end.
#[must_use]
pub fn supports_under_centred_rail(
    rail_length_m: f64,
    spacing_m: f64,
    min_overhang_m: f64,
) -> usize {
    if rail_length_m <= 2.0 * min_overhang_m || spacing_m <= 0.0 {
        return 0;
    }
    let usable = rail_length_m - 2.0 * min_overhang_m;
    (usable / spacing_m + 1e-9).floor() as usize + 1
}

/// Number of grid lines (seams, rafters) under a rail starting at `start_m`, for a grid with a
/// line at `grid_origin_m` and spacing `spacing_m`, keeping at least `min_edge_m` from both
/// rail ends. This is the exact count once the layout gives the grid position.
#[must_use]
pub fn supports_on_grid(
    start_m: f64,
    rail_length_m: f64,
    grid_origin_m: f64,
    spacing_m: f64,
    min_edge_m: f64,
) -> usize {
    if spacing_m <= 0.0 {
        return 0;
    }
    let lo = start_m + min_edge_m;
    let hi = start_m + rail_length_m - min_edge_m;
    if hi < lo {
        return 0;
    }
    let first = ((lo - grid_origin_m) / spacing_m - 1e-9).ceil();
    let last = ((hi - grid_origin_m) / spacing_m + 1e-9).floor();
    if last < first {
        0
    } else {
        (last - first) as usize + 1
    }
}

/// Climbers (top-rail fixations) of a field: each top rail rests on every bottom rail it
/// crosses; top rails are centred on the bottom rails.
#[must_use]
pub fn climber_count(top_rail_lengths_m: &[f64], bottom_rail_spacing_m: f64) -> usize {
    top_rail_lengths_m
        .iter()
        .map(|&l| supports_under_centred_rail(l, bottom_rail_spacing_m, 0.0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_counting() {
        // 3.06 m rail, 0.6 m seams, first seam 0.03 m after the rail start → 6 clamps.
        assert_eq!(supports_on_grid(0.0, 3.06, 0.03, 0.6, 0.0), 6);
        // Same rail shifted so that the first seam is 0.2 m in → 5 clamps.
        assert_eq!(supports_on_grid(0.0, 3.06, 0.2, 0.6, 0.0), 5);
        assert_eq!(supports_under_centred_rail(2.388, 1.0, 0.0), 3);
        assert_eq!(supports_under_centred_rail(1.234, 1.0, 0.0), 2);
        assert_eq!(supports_under_centred_rail(0.5, 1.0, 0.3), 0);
    }

    #[test]
    fn rafter_grid_picks_largest_multiple() {
        let system = CrossRailSystem {
            top_rail: RailSection::k2_single_rail_36(),
            bottom_rail: RailSection::k2_single_rail_36(),
            top_rail_tributary_width_m: 0.9,
            bottom_rail_spacing_m: 1.0,
            support_grid: SupportGrid::Rafters { spacing_m: 0.4 },
            actual_fixation_spacing_m: None,
            fixation: None,
        };
        let loads = ZoneSurfaceLoads {
            uls_pressure_perp: 1.0,
            uls_pressure_par: 0.2,
            uls_uplift_perp: -0.8,
            uls_uplift_par: 0.03,
            sls_pressure_perp: 0.7,
            sls_pressure_par: 0.15,
            sls_uplift_perp: -0.5,
            sls_uplift_par: 0.03,
        };
        let r = descend(&system, &loads).unwrap();
        let s = r.fixation_spacing.value;
        assert!((s / 0.4 - (s / 0.4).round()).abs() < 1e-9);
        assert!(s <= r.bottom_rail.max_span.value + 1e-9);
        assert!(s + 0.4 > r.bottom_rail.max_span.value);
    }
}
