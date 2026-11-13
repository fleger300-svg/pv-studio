//! DC wiring of a project: inverters (frozen inverter components), strings (ordered module
//! positions on one tracker of one inverter) and the design settings of the checks.
//!
//! Strings refer to module *positions* (array, row, col), not to module ids: ids are
//! renumbered when an array is laid out again, positions survive as long as the grid does.
//! A string whose positions no longer hold an active module is reported by the checks
//! (`pv-electrical`), never silently repaired.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::command::{keys, CommandError};
use crate::component::ComponentSpec;
use crate::ids::{ArrayId, ComponentId, InverterId, StringId};
use crate::project::Project;

/// Position of a module in its array's grid.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRef {
    pub array: ArrayId,
    pub row: u32,
    pub col: u32,
}

/// One inverter of the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InverterUnit {
    pub id: InverterId,
    /// Shown on plans and reports (« WR1 », « Onduleur garage »).
    pub name: String,
    /// Frozen inverter component.
    pub component: ComponentId,
}

/// A series string of modules on one tracker (MPPT) of an inverter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PvString {
    pub id: StringId,
    pub inverter: InverterId,
    /// Index of the tracker in the inverter's `dc.trackers`.
    pub tracker: u32,
    /// Modules in wiring order (first = plus pole).
    pub modules: Vec<ModuleRef>,
    /// Locked strings are never changed by the automatic stringing.
    #[serde(default)]
    pub locked: bool,
}

/// Design settings of the electrical checks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WiringSettings {
    /// Lowest cell temperature (open-circuit voltage check) [°C]; −10 °C is the usual Swiss
    /// value of PV*SOL.
    pub min_cell_temp_c: f64,
    /// Highest cell temperature (lower MPP voltage check) [°C].
    pub max_cell_temp_c: f64,
    /// Acceptable DC/AC power ratio of an inverter (installed Wp / AC W).
    pub dc_ac_min: f64,
    pub dc_ac_max: f64,
    /// Voltage factor of the string plan table (`Voc × kT`, 1.15 on the reference plan).
    pub voc_factor: f64,
}

impl Default for WiringSettings {
    fn default() -> Self {
        Self {
            min_cell_temp_c: -10.0,
            max_cell_temp_c: 70.0,
            dc_ac_min: 0.9,
            dc_ac_max: 1.3,
            voc_factor: 1.15,
        }
    }
}

/// Inverters, strings and settings of a project.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Wiring {
    pub inverters: Vec<InverterUnit>,
    pub strings: Vec<PvString>,
    #[serde(default)]
    pub settings: WiringSettings,
}

impl Wiring {
    /// Components the wiring refers to.
    #[must_use]
    pub fn component_ids(&self) -> Vec<ComponentId> {
        let mut ids: Vec<ComponentId> = self.inverters.iter().map(|i| i.component).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Structural validity against `project` (dangling module positions are allowed: the
    /// checks report them).
    ///
    /// # Errors
    /// [`keys::ENTITY_INVALID`] / [`keys::ENTITY_REFERENCE`] / component errors, with the
    /// offending field.
    pub fn validate(&self, project: &Project) -> Result<(), CommandError> {
        let invalid =
            |field: &str| CommandError::new(keys::ENTITY_INVALID).with_text("field", field);
        let s = &self.settings;
        if ![
            s.min_cell_temp_c,
            s.max_cell_temp_c,
            s.dc_ac_min,
            s.dc_ac_max,
            s.voc_factor,
        ]
        .iter()
        .all(|x| x.is_finite())
            || s.min_cell_temp_c >= s.max_cell_temp_c
            || !(-60.0..=40.0).contains(&s.min_cell_temp_c)
            || !(0.0..=120.0).contains(&s.max_cell_temp_c)
            || s.dc_ac_min <= 0.0
            || s.dc_ac_min >= s.dc_ac_max
            || !(1.0..=1.5).contains(&s.voc_factor)
        {
            return Err(invalid("settings"));
        }
        let mut ids = Vec::new();
        for inv in &self.inverters {
            if ids.contains(&inv.id) {
                return Err(invalid("inverter"));
            }
            ids.push(inv.id);
            let Some(snap) = project.component(inv.component) else {
                return Err(CommandError::new(keys::COMPONENT_UNKNOWN)
                    .with_text("id", inv.component.to_string()));
            };
            if !matches!(snap.component.spec, ComponentSpec::Inverter(_)) {
                return Err(CommandError::new(keys::COMPONENT_WRONG_FAMILY)
                    .with_text("id", inv.component.to_string()));
            }
        }
        let mut string_ids = Vec::new();
        let mut used = std::collections::HashSet::new();
        for s in &self.strings {
            if string_ids.contains(&s.id) || s.modules.is_empty() {
                return Err(invalid("string"));
            }
            string_ids.push(s.id);
            let Some(inv) = self.inverters.iter().find(|i| i.id == s.inverter) else {
                return Err(CommandError::new(keys::ENTITY_REFERENCE).with_text("kind", "inverter"));
            };
            let trackers = project
                .component(inv.component)
                .and_then(|c| match &c.component.spec {
                    ComponentSpec::Inverter(i) => Some(i.dc.trackers.len()),
                    _ => None,
                })
                .unwrap_or(0);
            if s.tracker as usize >= trackers {
                return Err(invalid("tracker"));
            }
            for m in &s.modules {
                if !used.insert(*m) {
                    return Err(invalid("modules"));
                }
            }
        }
        Ok(())
    }
}
