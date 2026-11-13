//! Electrical components: inverters, optimizers, batteries and other equipment. Values as
//! printed on the datasheets; `None` when the datasheet does not give them.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{non_negative, positive};

/// Datasheet facts shared by every electrical component (not used by the engines).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct EquipmentDetails {
    #[specta(optional)]
    pub series: Option<String>,
    #[specta(optional)]
    pub ip_rating: Option<String>,
    #[specta(optional)]
    pub mass_kg: Option<f64>,
    /// Width × height × depth [mm].
    #[specta(optional)]
    pub dimensions_mm: Option<[f64; 3]>,
    #[specta(optional)]
    pub operating_temp_min_c: Option<f64>,
    #[specta(optional)]
    pub operating_temp_max_c: Option<f64>,
    pub communication: Vec<String>,
    #[specta(optional)]
    pub datasheet_ref: Option<String>,
    pub notes: Vec<String>,
}

// --- Inverters -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum InverterKind {
    /// String inverter (PV only).
    String,
    /// Hybrid: PV and a battery port.
    Hybrid,
    Micro,
    /// Fixed-voltage inverter driven by module optimizers (SolarEdge): string rules come from
    /// the optimizer.
    OptimizerBased,
}

/// One MPP tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Tracker {
    /// String inputs on this tracker.
    pub inputs: u32,
    #[specta(optional)]
    pub max_current_a: Option<f64>,
    #[specta(optional)]
    pub max_short_circuit_current_a: Option<f64>,
    #[specta(optional)]
    pub max_power_w: Option<f64>,
}

/// AC side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InverterAc {
    pub phases: u8,
    pub nominal_power_w: f64,
    #[specta(optional)]
    pub max_apparent_power_va: Option<f64>,
    #[specta(optional)]
    pub max_active_power_w: Option<f64>,
    #[specta(optional)]
    pub nominal_voltage_v: Option<f64>,
    #[specta(optional)]
    pub max_current_a: Option<f64>,
}

/// DC side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InverterDc {
    /// Absolute maximum input voltage [V].
    pub max_voltage_v: f64,
    #[specta(optional)]
    pub start_voltage_v: Option<f64>,
    #[specta(optional)]
    pub min_voltage_v: Option<f64>,
    #[specta(optional)]
    pub nominal_voltage_v: Option<f64>,
    /// MPP window [V] (fixed operating voltage for optimizer-based inverters).
    pub mppt_min_v: f64,
    pub mppt_max_v: f64,
    /// MPP window at full power, when printed.
    #[specta(optional)]
    pub full_power_mppt_min_v: Option<f64>,
    #[specta(optional)]
    pub full_power_mppt_max_v: Option<f64>,
    /// Maximum DC (PV) power; its meaning differs by manufacturer (see `max_power_basis`).
    #[specta(optional)]
    pub max_power_w: Option<f64>,
    #[specta(optional)]
    pub max_power_basis: Option<String>,
    #[specta(optional)]
    pub max_current_total_a: Option<f64>,
    #[specta(optional)]
    pub max_short_circuit_current_total_a: Option<f64>,
    /// One per MPPT (asymmetric trackers are common).
    pub trackers: Vec<Tracker>,
}

/// Battery port of a hybrid inverter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatteryPort {
    #[specta(optional)]
    pub voltage_min_v: Option<f64>,
    #[specta(optional)]
    pub voltage_max_v: Option<f64>,
    #[specta(optional)]
    pub max_current_a: Option<f64>,
    #[specta(optional)]
    pub max_charge_power_w: Option<f64>,
    #[specta(optional)]
    pub max_discharge_power_w: Option<f64>,
    /// Battery models named by the datasheet.
    pub compatible: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InverterSpec {
    pub kind: InverterKind,
    pub ac: InverterAc,
    pub dc: InverterDc,
    #[specta(optional)]
    pub battery: Option<BatteryPort>,
    /// Backup / emergency power, as described by the datasheet.
    #[specta(optional)]
    pub backup: Option<String>,
    pub optimizer_required: bool,
    pub compatible_optimizers: Vec<String>,
    #[specta(optional)]
    pub max_efficiency_pct: Option<f64>,
    #[specta(optional)]
    pub euro_efficiency_pct: Option<f64>,
    /// Arc-fault detection: « yes », « no », « optional ».
    #[specta(optional)]
    pub afci: Option<String>,
    #[specta(optional)]
    pub spd_dc: Option<String>,
    #[specta(optional)]
    pub spd_ac: Option<String>,
    #[serde(default)]
    pub details: EquipmentDetails,
}

impl InverterSpec {
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        let d = &self.dc;
        if !positive(self.ac.nominal_power_w) || self.ac.phases == 0 || self.ac.phases > 3 {
            Some("ac")
        } else if !positive(d.max_voltage_v) {
            Some("dc.maxVoltageV")
        } else if !(positive(d.mppt_min_v)
            && d.mppt_min_v <= d.mppt_max_v
            && d.mppt_max_v <= d.max_voltage_v)
        {
            Some("dc.mppt")
        } else if d.trackers.is_empty() || d.trackers.iter().any(|t| t.inputs == 0) {
            Some("dc.trackers")
        } else {
            None
        }
    }
}

// --- Optimizers ------------------------------------------------------------------------------

/// String length rule of an optimizer with a given inverter family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerStringRule {
    /// Inverter model or family pattern (e.g. « SE*K », « SUN2000-*KTL-M1 »).
    pub inverter: String,
    #[specta(optional)]
    pub min_optimizers: Option<u32>,
    #[specta(optional)]
    pub max_optimizers: Option<u32>,
    #[specta(optional)]
    pub max_power_per_string_w: Option<f64>,
    #[specta(optional)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerSpec {
    #[specta(optional)]
    pub modules_per_optimizer: Option<u32>,
    pub rated_input_power_w: f64,
    pub max_input_voltage_v: f64,
    #[specta(optional)]
    pub mppt_min_v: Option<f64>,
    #[specta(optional)]
    pub mppt_max_v: Option<f64>,
    #[specta(optional)]
    pub max_input_short_circuit_current_a: Option<f64>,
    #[specta(optional)]
    pub max_output_current_a: Option<f64>,
    #[specta(optional)]
    pub max_output_voltage_v: Option<f64>,
    #[specta(optional)]
    pub max_efficiency_pct: Option<f64>,
    #[specta(optional)]
    pub weighted_efficiency_pct: Option<f64>,
    pub compatible_inverters: Vec<String>,
    pub string_rules: Vec<OptimizerStringRule>,
    #[serde(default)]
    pub details: EquipmentDetails,
}

impl OptimizerSpec {
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        if !positive(self.rated_input_power_w) {
            Some("ratedInputPowerW")
        } else if !positive(self.max_input_voltage_v) {
            Some("maxInputVoltageV")
        } else {
            None
        }
    }
}

// --- Batteries -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BatteryCoupling {
    /// On the battery port of a hybrid inverter.
    Dc,
    /// With its own power conversion system.
    Ac,
}

/// How the datasheet gives the battery power.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum BatteryPowerRule {
    /// Power per module (Huawei LUNA S1).
    PerModule { charge_w: f64, discharge_w: f64 },
    /// Current limit: power = current × configuration voltage (BYD, Fronius, Dyness).
    CurrentLimited { max_current_a: f64 },
    /// C-rate on the configuration's energy (C&I cabinets).
    CRate { max_c: f64 },
    /// Fixed power of the system (AC cabinets).
    Fixed { charge_w: f64, discharge_w: f64 },
    /// Given per configuration (see `configurations`).
    PerConfiguration,
    /// Limited by the inverter only.
    InverterLimited,
}

/// One configuration printed by the datasheet (number of modules).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatteryConfiguration {
    pub modules: u32,
    #[specta(optional)]
    pub usable_kwh: Option<f64>,
    #[specta(optional)]
    pub nominal_kwh: Option<f64>,
    #[specta(optional)]
    pub voltage_v: Option<f64>,
    #[specta(optional)]
    pub max_power_w: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatterySpec {
    /// « LFP », « NMC »…
    pub chemistry: String,
    pub coupling: BatteryCoupling,
    #[specta(optional)]
    pub module_model: Option<String>,
    #[specta(optional)]
    pub module_usable_kwh: Option<f64>,
    #[specta(optional)]
    pub module_nominal_kwh: Option<f64>,
    pub min_modules: u32,
    pub max_modules_per_stack: u32,
    #[specta(optional)]
    pub max_stacks: Option<u32>,
    pub power: BatteryPowerRule,
    pub configurations: Vec<BatteryConfiguration>,
    #[specta(optional)]
    pub voltage_min_v: Option<f64>,
    #[specta(optional)]
    pub voltage_max_v: Option<f64>,
    #[specta(optional)]
    pub depth_of_discharge_pct: Option<f64>,
    /// Round-trip efficiency, with its basis in `notes` (DC-DC or AC-AC with auxiliaries).
    #[specta(optional)]
    pub round_trip_efficiency_pct: Option<f64>,
    #[specta(optional)]
    pub cycles: Option<f64>,
    #[specta(optional)]
    pub warranty_years: Option<f64>,
    /// « indoor », « outdoor », « both ».
    #[specta(optional)]
    pub location: Option<String>,
    #[specta(optional)]
    pub mass_kg_per_module: Option<f64>,
    pub compatible_inverters: Vec<String>,
    #[serde(default)]
    pub details: EquipmentDetails,
}

impl BatterySpec {
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        if self.min_modules == 0 || self.max_modules_per_stack < self.min_modules {
            Some("modules")
        } else if (self.module_usable_kwh.or(self.module_nominal_kwh).is_none()
            && self.configurations.is_empty())
            || [self.module_usable_kwh, self.module_nominal_kwh]
                .into_iter()
                .flatten()
                .any(|x| !positive(x))
        {
            Some("capacity")
        } else {
            None
        }
    }
}

// --- Other equipment -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EquipmentKind {
    EnergyManager,
    BackupBox,
    DcBox,
    EvCharger,
    Cable,
    Connector,
    CableTray,
    Communication,
    SafetyLine,
    Carport,
    Other,
}

/// A printed characteristic: a number with its unit, or a text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PropertyValue {
    Number { value: f64, unit: String },
    Text { value: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub name: String,
    pub value: PropertyValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentSpec {
    pub kind: EquipmentKind,
    pub properties: Vec<Property>,
    pub compatible_with: Vec<String>,
    #[serde(default)]
    pub details: EquipmentDetails,
}

impl EquipmentSpec {
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        self.properties
            .iter()
            .any(|p| matches!(&p.value, PropertyValue::Number { value, .. } if !non_negative(value.abs())))
            .then_some("properties")
    }
}
