//! `SimResults.xml`: PV*SOL's simulation results — the energy balance (irradiance, PV and
//! inverter steps), the grid feed-in and the results per module area.
//!
//! Energies are stored in joules (W·s) and irradiation in J/m²; both are converted to kWh
//! and kWh/m² here. Each balance group repeats once for the whole system, then once per
//! area / MPP tracker / inverter: the first block is the total.

use serde::Serialize;
use specta::Type;

use crate::xml::Node;
use crate::ReadError;

const J_PER_KWH: f64 = 3.6e6;

/// Irradiation steps per square metre of module [kWh/m²] (signed: losses negative).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IrradianceBalance {
    pub global_horizontal: f64,
    pub spectral: f64,
    /// Gain of the tilted plane over horizontal (transposition).
    pub tilt_gain: f64,
    pub ground_reflection: f64,
    /// Far shading (horizon).
    pub horizon_shading: f64,
    /// Reflection at the module surface (incidence angle).
    pub reflection: f64,
    pub on_module: f64,
}

/// PV generator steps [kWh] (signed: losses negative).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PvBalance {
    /// Irradiation on the module area.
    pub irradiation: f64,
    pub soiling: f64,
    pub stc_conversion: f64,
    /// Nominal energy (STC efficiency).
    pub nominal: f64,
    /// Near shading of the modules (3-D scene).
    pub module_shading: f64,
    /// Low-light (part-load) behaviour.
    pub part_load: f64,
    pub temperature: f64,
    pub diodes: f64,
    pub mismatch: f64,
    pub mismatch_wiring: f64,
    pub string_cables: f64,
    pub dc_cables: f64,
    /// Clipping and MPP adaptation at the inverter input.
    pub inverter_input_limits: f64,
    pub dc_energy: f64,
}

/// Inverter steps [kWh] (signed: losses negative).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InverterBalance {
    pub input: f64,
    pub voltage_deviation: f64,
    pub conversion: f64,
    pub standby: f64,
    pub ac_cables: f64,
    pub ac_energy: f64,
    pub fed_in: f64,
}

/// Results of one module area.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AreaResult {
    pub area: String,
    pub performance_ratio: f64,
    pub ac_energy_kwh: f64,
}

/// PV*SOL's results.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PvsolResults {
    pub irradiance: IrradianceBalance,
    pub pv: PvBalance,
    pub inverter: InverterBalance,
    /// Grid feed-in [kWh], year and months.
    pub grid_feed_in_kwh: f64,
    pub monthly_feed_in_kwh: Vec<f64>,
    /// Global horizontal irradiation by month [kWh/m²].
    pub monthly_ghi_kwh_m2: Vec<f64>,
    /// Installed DC power [kWp] (sum over the inverters).
    pub dc_power_kwp: f64,
    pub areas: Vec<AreaResult>,
}

impl PvsolResults {
    /// Specific yield [kWh/kWp] (fed-in energy over installed power).
    #[must_use]
    pub fn specific_yield(&self) -> f64 {
        if self.dc_power_kwp > 0.0 {
            self.grid_feed_in_kwh.max(self.inverter.fed_in) / self.dc_power_kwp
        } else {
            0.0
        }
    }
}

fn yearly(n: &Node, name: &str) -> f64 {
    n.child(name)
        .and_then(|c| c.f64_at("YearlyValue"))
        .unwrap_or(0.0)
}

fn monthly(n: &Node, name: &str, div: f64) -> Vec<f64> {
    n.child(name).map_or_else(Vec::new, |c| {
        c.children("MonthlyValues")
            .map(|m| m.f64_at("value").unwrap_or(0.0) / div)
            .collect()
    })
}

/// Reads `SimResults.xml`; `None` when it holds no energy balance (project saved without a
/// simulation, or a system type without one).
///
/// # Errors
/// [`ReadError::Format`] when the root element is not the results.
pub fn parse(doc: &Node) -> Result<Option<PvsolResults>, ReadError> {
    if doc.name != "SimResultsXML" {
        return Err(ReadError::Format {
            file: "SimResults.xml".into(),
            what: "SimResultsXML".into(),
        });
    }
    let (Some(meteo), Some(pv), Some(wr)) = (
        doc.child("EnergiebilanzResultsPVFeld"),
        doc.child("EnergiebilanzResultsVerschaltung"),
        doc.child("EnergiebilanzResultsWr"),
    ) else {
        return Ok(None);
    };
    let m = |name: &str| yearly(meteo, name) / J_PER_KWH;
    let p = |name: &str| yearly(pv, name) / J_PER_KWH;
    let w = |name: &str| yearly(wr, name) / J_PER_KWH;

    let irradiance = IrradianceBalance {
        global_horizontal: m("EB_Meteo0_GlobalstrahlungHorizontal"),
        spectral: m("EB_Meteo1_AbweichungVomStandardspektrum"),
        tilt_gain: m("EB_Meteo2_Aufständerung"),
        ground_reflection: m("EB_Meteo2_a_Bodenreflexion"),
        horizon_shading: m("EB_Meteo3_Verschattung"),
        reflection: m("EB_Meteo4_Reflexion"),
        on_module: m("EB_Meteo5_GlobalstrahlungAufModul"),
    };
    let pv_balance = PvBalance {
        irradiation: p("EB_PV0_PVGlobalstrahlung"),
        soiling: p("EB_PV1_Verschmutzung"),
        stc_conversion: p("EB_PV2_STCKonversion"),
        nominal: p("EB_PV3_PVNennenergie"),
        module_shading: p("EB_PV4_3D_Modulabschattung"),
        part_load: p("EB_PV4_Teillast"),
        temperature: p("EB_PV5_Temperatur"),
        diodes: p("EB_PV6_DiodenVerluste"),
        mismatch: p("EB_PV7_MismatchUser"),
        mismatch_wiring: p("EB_PV8_MismatchVerschaltung") + p("EB_PV8a_PowerOptimizerVerluste"),
        string_cables: p("EB_PV9_Stringverkabelung"),
        dc_cables: p("EB_PV10_VerkabelungDC"),
        inverter_input_limits: p("EB_PV12_AbregelungU")
            + p("EB_PV12c_AbregelungPDcStart")
            + p("EB_PV13_AbregelungI")
            + p("EB_PV14_AbregelungPDC")
            + p("EB_PV15_AbregelungPAC")
            + p("EB_PV16_MPPAnpassung"),
        dc_energy: p("EB_PV17_ErzeugtePVEnergie"),
    };
    let inverter = InverterBalance {
        input: w("EB_WR0_EingangsEnergie"),
        voltage_deviation: w("EB_WR1_AbweichungUzuUnenn"),
        conversion: w("EB_WR2_DCAC"),
        standby: w("EB_WR3_Eigenverbrauch"),
        ac_cables: w("EB_WR4_VerkabelungAC"),
        ac_energy: w("EB_WR5_PVEnergieAC"),
        fed_in: w("EB_WR6_EingespeisteEnergie"),
    };

    let usage = doc.child("VerbrauchResults");
    let feed = usage.map_or(0.0, |u| yearly(u, "Netzeinspeisung") / J_PER_KWH);
    let monthly_feed = usage.map_or_else(Vec::new, |u| monthly(u, "Netzeinspeisung", J_PER_KWH));
    let monthly_ghi = doc
        .child("MeteoResults")
        .map_or_else(Vec::new, |mr| monthly(mr, "EinstrahlungHor", J_PER_KWH));
    let dc_power_kwp = doc
        .children("InverterResult")
        .filter_map(|i| i.f64_at("DCPower"))
        .sum();
    let areas = doc
        .children("ModulflaechenResults")
        .map(|a| AreaResult {
            area: crate::guid::normalise(a.text_at("Guid_Area").unwrap_or_default()),
            performance_ratio: yearly(a, "PerformanceRatio"),
            ac_energy_kwh: yearly(a, "PVEnergyAC"),
        })
        .collect();
    Ok(Some(PvsolResults {
        irradiance,
        pv: pv_balance,
        inverter,
        grid_feed_in_kwh: feed,
        monthly_feed_in_kwh: monthly_feed,
        monthly_ghi_kwh_m2: monthly_ghi,
        dc_power_kwp,
        areas,
    }))
}
