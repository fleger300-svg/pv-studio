//! PV Studio's simulation of an imported project next to PV*SOL's own results.
//!
//! The losses PV*SOL takes as *inputs* (soiling, spectral, mismatch, diodes, cables) are set
//! to PV*SOL's values ([`assumptions_like`]), so that the comparison tests what PV Studio
//! *computes*: transposition, shading, reflection, temperature, inverter and clipping.
//!
//! The two balances are not booked in the same order (PV*SOL books horizon shading and
//! reflection on the irradiance, then soiling, then module shading on the energy), so each
//! step is compared as a **relative loss** of the energy that reaches it.

use pv_model::Project;
use pv_solar::{simulate, SimulationAssumptions, YieldReport};
use serde::Serialize;
use specta::Type;

use crate::{PvsolFile, PvsolResults};

/// One compared quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonRow {
    /// Translation key suffix (`pvsol.compare.<key>`).
    pub key: String,
    /// `kWh`, `kWh/m²`, `kWh/kWp`, `%`, `kWp`.
    pub unit: String,
    pub pvsol: f64,
    pub pvstudio: f64,
}

/// The comparison of one project.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub rows: Vec<ComparisonRow>,
    pub monthly_ac_pvsol_kwh: Vec<f64>,
    pub monthly_ac_pvstudio_kwh: Vec<f64>,
    /// Both simulations ran on PV*SOL's weather file.
    pub same_weather: bool,
    pub assumptions: SimulationAssumptions,
}

impl Comparison {
    /// Relative gap of the annual AC energy (PV Studio vs PV*SOL).
    #[must_use]
    pub fn energy_gap(&self) -> Option<f64> {
        let r = self.rows.iter().find(|r| r.key == "acEnergy")?;
        (r.pvsol > 0.0).then(|| r.pvstudio / r.pvsol - 1.0)
    }
}

fn mean_area_losses(file: &PvsolFile) -> (f64, f64, f64, f64) {
    let areas = &file.config.areas;
    let n: f64 = areas.iter().map(|a| f64::from(a.module_count)).sum();
    if n <= 0.0 {
        return (0.2, 0.0, 0.0, 0.0);
    }
    let avg = |f: &dyn Fn(&crate::config::AreaLosses) -> f64| {
        areas
            .iter()
            .map(|a| f(&a.losses) * f64::from(a.module_count))
            .sum::<f64>()
            / n
    };
    (
        avg(&|l| l.albedo),
        avg(&|l| l.soiling),
        avg(&|l| l.mismatch),
        avg(&|l| l.diodes),
    )
}

/// PV Studio's assumptions with PV*SOL's input losses: albedo, soiling (with the spectral
/// loss, which PV Studio does not model separately), mismatch (with the diodes), and the
/// cable losses PV*SOL booked (string, DC and AC cables over the inverter input).
#[must_use]
pub fn assumptions_like(file: &PvsolFile) -> SimulationAssumptions {
    let (albedo, soiling, mismatch, diodes) = mean_area_losses(file);
    let spectral = file.config.spectral_loss;
    let cables = file.results.as_ref().map_or(0.0, |r| {
        let lost = -(r.pv.string_cables + r.pv.dc_cables + r.inverter.ac_cables);
        if r.inverter.input > 0.0 {
            (lost / r.inverter.input).clamp(0.0, 0.2)
        } else {
            0.0
        }
    });
    SimulationAssumptions {
        albedo,
        soiling_loss: 1.0 - (1.0 - soiling) * (1.0 - spectral),
        mismatch_loss: 1.0 - (1.0 - mismatch) * (1.0 - diodes),
        dc_cable_loss: cables,
        ..SimulationAssumptions::default()
    }
}

fn ratio(loss: f64, of: f64) -> f64 {
    if of > 0.0 {
        100.0 * loss / of
    } else {
        0.0
    }
}

fn row(key: &str, unit: &str, pvsol: f64, pvstudio: f64) -> ComparisonRow {
    ComparisonRow {
        key: key.to_owned(),
        unit: unit.to_owned(),
        pvsol,
        pvstudio,
    }
}

/// Builds the rows from both results.
#[must_use]
pub fn rows(p: &PvsolResults, s: &YieldReport) -> Vec<ComparisonRow> {
    let i = &p.irradiance;
    let b = &s.balance;
    // Irradiance on the tilted plane before any shading or reflection.
    let poa_pvsol = i.global_horizontal + i.tilt_gain + i.ground_reflection;
    // Shading: horizon (on the irradiance) then modules (on the energy), combined.
    let horizon = -i.horizon_shading / poa_pvsol.max(1e-9);
    let modules = -p.pv.module_shading / p.pv.nominal.max(1e-9);
    let shading_pvsol = 100.0 * (1.0 - (1.0 - horizon) * (1.0 - modules));
    let after_horizon = poa_pvsol + i.horizon_shading;
    let reflection_pvsol = ratio(-i.reflection, after_horizon);
    // PV*SOL's temperature step follows module shading and part load.
    let before_temp_pvsol = p.pv.nominal + p.pv.module_shading + p.pv.part_load;
    let before_temp = b.nominal_kwh - b.shading_kwh - b.iam_kwh - b.soiling_kwh - b.low_light_kwh;
    let inverter_pvsol =
        -(p.inverter.voltage_deviation + p.inverter.conversion + p.inverter.standby);
    let dc_pvstudio = before_temp - b.temperature_kwh - b.mismatch_kwh - b.dc_cable_kwh;
    vec![
        row("peakPower", "kWp", p.dc_power_kwp, s.peak_power_kwp),
        row("ghi", "kWh/m²", i.global_horizontal, s.ghi_kwh_m2),
        row("poa", "kWh/m²", poa_pvsol, s.poa_kwh_m2),
        row(
            "shading",
            "%",
            shading_pvsol,
            ratio(b.shading_kwh, b.nominal_kwh),
        ),
        // Its two parts: the far horizon (relief; not modelled by PV Studio yet) and the near
        // shading (buildings, obstacles, surroundings).
        row("horizonShading", "%", 100.0 * horizon, 0.0),
        row(
            "nearShading",
            "%",
            100.0 * modules,
            ratio(b.shading_kwh, b.nominal_kwh),
        ),
        row(
            "reflection",
            "%",
            reflection_pvsol,
            ratio(b.iam_kwh, b.nominal_kwh - b.shading_kwh),
        ),
        row(
            "lowLight",
            "%",
            ratio(-p.pv.part_load, p.pv.nominal + p.pv.module_shading),
            ratio(
                b.low_light_kwh,
                b.nominal_kwh - b.shading_kwh - b.iam_kwh - b.soiling_kwh,
            ),
        ),
        row(
            "temperature",
            "%",
            ratio(-p.pv.temperature, before_temp_pvsol),
            ratio(b.temperature_kwh, before_temp),
        ),
        // Unequal strings in parallel on one tracker (PV*SOL's « MismatchVerschaltung »):
        // not modelled by PV Studio yet.
        row(
            "wiringMismatch",
            "%",
            ratio(
                -p.pv.mismatch_wiring,
                before_temp_pvsol + p.pv.temperature + p.pv.diodes + p.pv.mismatch,
            ),
            0.0,
        ),
        row(
            "inverter",
            "%",
            ratio(inverter_pvsol, p.inverter.input),
            ratio(b.inverter_kwh, dc_pvstudio),
        ),
        row(
            "clipping",
            "%",
            ratio(
                -p.pv.inverter_input_limits,
                p.pv.dc_energy - p.pv.inverter_input_limits,
            ),
            ratio(b.clipping_kwh, dc_pvstudio),
        ),
        row(
            "acEnergy",
            "kWh",
            p.grid_feed_in_kwh.max(p.inverter.fed_in),
            s.ac_energy_kwh,
        ),
        row(
            "specificYield",
            "kWh/kWp",
            p.specific_yield(),
            s.specific_yield,
        ),
    ]
}

/// Simulates `project` (imported from `file`) with PV*SOL-like input losses and sets it next
/// to PV*SOL's results; `None` when PV*SOL has no results or the project cannot be simulated.
#[must_use]
pub fn compare(file: &PvsolFile, project: &Project) -> Option<Comparison> {
    compare_with(file, project, assumptions_like(file))
}

/// [`compare`] with given assumptions (calibration studies).
#[must_use]
pub fn compare_with(
    file: &PvsolFile,
    project: &Project,
    a: SimulationAssumptions,
) -> Option<Comparison> {
    let p = file.results.as_ref()?;
    let s = simulate(project, &a).ok()?;
    let same_weather = file.weather.as_ref().is_some_and(|(name, _)| {
        project
            .site
            .weather
            .as_ref()
            .is_some_and(|w| w.source.contains(name.as_str()))
    });
    Some(Comparison {
        rows: rows(p, &s),
        monthly_ac_pvsol_kwh: p.monthly_feed_in_kwh.clone(),
        monthly_ac_pvstudio_kwh: s.monthly_ac_kwh.clone(),
        same_weather,
        assumptions: a,
    })
}
