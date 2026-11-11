//! Mounting systems: statics data, layout defaults and the parts they are made of, by
//! reference to [`super::PartSpec`] components.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{non_negative, positive, LengthRange};
use crate::ids::ComponentId;
use crate::layout::{ArrayLayoutParams, SupportGridKind};
use crate::mounting::{BallastSpec, CpSpec, Driver};

/// Statics data common to all families.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatics {
    /// Mounting system mass per module, without ballast [kg].
    pub system_mass_per_module_kg: f64,
    /// Loaded area for the pressure coefficients [m²].
    pub loaded_area_m2: f64,
    /// Combination factor ψ₀ for snow, when the manufacturer's tool deviates from SIA 260.
    #[specta(optional)]
    pub psi0_snow: Option<f64>,
    pub cp: CpSpec,
}

/// Structural family, with the data that only exists in that family (a cross-rail system has
/// rails and no ballast, a ballasted system the reverse).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MountingStructure {
    /// Top rails on bottom rails, fixed to the roof (coplanar fields).
    CrossRail {
        /// Rail parts ([`super::PartKind::Rail`]) of each layer.
        top_rail: ComponentId,
        bottom_rail: ComponentId,
        /// Fixation part checked by the statics; `None`: fixations not checked (e.g. K2
        /// standing-seam clamps, one on every seam).
        #[specta(optional)]
        checked_fixation: Option<ComponentId>,
    },
    /// Aerodynamic ballasted system (east-west on flat roofs).
    Ballasted { ballast: BallastSpec },
}

/// One part of a system: a part component, counted by `driver`, `per` pieces each time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PartRuleRef {
    pub part: ComponentId,
    pub driver: Driver,
    /// Pieces per counted item (ignored for rail bars).
    pub per: u32,
}

/// What a system can be mounted on and with.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemCompatibility {
    /// Roof support grids the system fixes to; empty = no fixation (ballasted).
    pub supports: Vec<SupportGridKind>,
    /// Accepted module long side / short side; `None` = no limit.
    #[specta(optional)]
    pub module_length: Option<LengthRange>,
    #[specta(optional)]
    pub module_width: Option<LengthRange>,
}

/// Type approval or technical assessment of a system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    /// Document reference (e.g. ETA, abZ, test report number).
    pub reference: String,
    pub issued_by: String,
}

/// A mounting system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MountingSystemSpec {
    pub structure: MountingStructure,
    /// Layout parameters a new array starts with (gaps, setbacks, rail geometry…).
    pub defaults: ArrayLayoutParams,
    pub statics: SystemStatics,
    /// Bill-of-materials rules, in report order.
    pub parts: Vec<PartRuleRef>,
    pub compatibility: SystemCompatibility,
    #[specta(optional)]
    pub approval: Option<Approval>,
}

impl MountingSystemSpec {
    /// Every part component the system refers to (rules, rails, checked fixation), in order,
    /// without duplicates: the closure to freeze into a project, besides the module.
    #[must_use]
    pub fn part_ids(&self) -> Vec<ComponentId> {
        let mut ids: Vec<ComponentId> = Vec::new();
        let mut push = |id: ComponentId| {
            if !ids.contains(&id) {
                ids.push(id);
            }
        };
        if let MountingStructure::CrossRail {
            top_rail,
            bottom_rail,
            checked_fixation,
        } = &self.structure
        {
            push(*top_rail);
            push(*bottom_rail);
            if let Some(f) = checked_fixation {
                push(*f);
            }
        }
        for rule in &self.parts {
            push(rule.part);
        }
        ids
    }

    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        let s = &self.statics;
        if !non_negative(s.system_mass_per_module_kg) {
            Some("systemMassPerModuleKg")
        } else if !positive(s.loaded_area_m2) {
            Some("loadedAreaM2")
        } else if s.psi0_snow.is_some_and(|p| !(non_negative(p) && p <= 1.0)) {
            Some("psi0Snow")
        } else if !self.defaults.boundary_uv.is_empty() || self.defaults.paired_with.is_some() {
            // Defaults are field-independent: no outline, no partner array.
            Some("defaults")
        } else if self
            .parts
            .iter()
            .any(|r| r.per == 0 && !matches!(r.driver, Driver::RailBars { .. }))
        {
            Some("per")
        } else {
            None
        }
    }
}
