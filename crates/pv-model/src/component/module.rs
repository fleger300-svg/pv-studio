//! PV modules (datasheet values, STC).

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{non_negative, positive};

/// Electrical datasheet values at STC.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModuleElectrical {
    pub pmax_w: f64,
    pub voc_v: f64,
    pub isc_a: f64,
    pub vmpp_v: f64,
    pub impp_a: f64,
    /// Temperature coefficients [%/K] (negative for Pmax and Voc).
    pub temp_coeff_pmax_pct_per_k: f64,
    pub temp_coeff_voc_pct_per_k: f64,
    pub temp_coeff_isc_pct_per_k: f64,
    /// Maximum system voltage [V] (IEC: 1000 or 1500).
    pub max_system_voltage_v: f64,
    /// Rear/front power ratio [-]; `None` for a monofacial module.
    #[specta(optional)]
    pub bifaciality: Option<f64>,
}

impl ModuleElectrical {
    fn invalid_field(&self) -> Option<&'static str> {
        let checks = [
            ("pmaxW", positive(self.pmax_w)),
            ("vocV", positive(self.voc_v)),
            ("iscA", positive(self.isc_a)),
            ("vmppV", positive(self.vmpp_v) && self.vmpp_v < self.voc_v),
            ("imppA", positive(self.impp_a) && self.impp_a < self.isc_a),
            (
                "tempCoeffPmaxPctPerK",
                self.temp_coeff_pmax_pct_per_k.is_finite(),
            ),
            (
                "tempCoeffVocPctPerK",
                self.temp_coeff_voc_pct_per_k.is_finite(),
            ),
            (
                "tempCoeffIscPctPerK",
                self.temp_coeff_isc_pct_per_k.is_finite(),
            ),
            ("maxSystemVoltageV", positive(self.max_system_voltage_v)),
            (
                "bifaciality",
                self.bifaciality.is_none_or(|b| non_negative(b) && b <= 1.0),
            ),
        ];
        checks.into_iter().find(|(_, ok)| !ok).map(|(f, _)| f)
    }
}

/// A PV module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModuleSpec {
    /// Long side [m].
    pub length_m: f64,
    /// Short side [m].
    pub width_m: f64,
    /// Frame height [m] (what clamps are rated for).
    pub thickness_m: f64,
    pub mass_kg: f64,
    /// `None` until the electrical values are entered (not needed for layout, statics and
    /// bill of materials; required for stringing).
    #[specta(optional)]
    pub electrical: Option<ModuleElectrical>,
    /// Further datasheet data (simulation, warranties, approvals); all optional.
    #[serde(default)]
    pub details: Box<ModuleDetails>,
}

/// How a module is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ModuleKind {
    /// Framed module on a mounting system.
    #[default]
    Framed,
    /// Building-integrated (replaces the roof covering: solar slates, tiles, Solrif frames).
    IntegratedRoof,
}

/// Datasheet data beyond what layout, statics and stringing need. Every value as printed;
/// `None` when the datasheet does not give it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ModuleDetails {
    pub kind: ModuleKind,
    /// Product series (e.g. « Hi-MO X10 »).
    #[specta(optional)]
    pub series: Option<String>,
    #[specta(optional)]
    pub cell_type: Option<String>,
    #[specta(optional)]
    pub cell_count: Option<u32>,
    /// `Some(false)`: monofacial; `None`: unknown.
    #[specta(optional)]
    pub bifacial: Option<bool>,
    /// Efficiency at STC, as printed [%].
    #[specta(optional)]
    pub efficiency_pct: Option<f64>,
    #[specta(optional)]
    pub max_series_fuse_a: Option<f64>,
    /// Nominal operating cell temperature (NOCT or NMOT) [°C].
    #[specta(optional)]
    pub nominal_operating_temp_c: Option<f64>,
    #[specta(optional)]
    pub operating_temp_min_c: Option<f64>,
    #[specta(optional)]
    pub operating_temp_max_c: Option<f64>,
    /// Test loads, front (snow) and back (wind suction) [Pa].
    #[specta(optional)]
    pub test_load_front_pa: Option<f64>,
    #[specta(optional)]
    pub test_load_back_pa: Option<f64>,
    /// VKF/AEAI hail class (e.g. « HW4 »).
    #[specta(optional)]
    pub hail_class: Option<String>,
    #[specta(optional)]
    pub fire_class: Option<String>,
    #[specta(optional)]
    pub warranty_product_years: Option<f64>,
    #[specta(optional)]
    pub warranty_power_years: Option<f64>,
    #[specta(optional)]
    pub degradation_first_year_pct: Option<f64>,
    #[specta(optional)]
    pub degradation_annual_pct: Option<f64>,
    /// Datasheet file the values come from, and its revision when printed.
    #[specta(optional)]
    pub datasheet_ref: Option<String>,
    /// Points to check (printed anomalies, ambiguities).
    pub notes: Vec<String>,
}

impl ModuleSpec {
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        if !positive(self.length_m) {
            Some("lengthM")
        } else if !positive(self.width_m) || self.width_m > self.length_m {
            Some("widthM")
        } else if !positive(self.thickness_m) {
            Some("thicknessM")
        } else if !positive(self.mass_kg) {
            Some("massKg")
        } else {
            self.electrical
                .as_ref()
                .and_then(ModuleElectrical::invalid_field)
        }
    }
}
