//! Library components: modules, mounting systems and mounting parts.
//!
//! A [`Component`] is one revision of a catalogue entry. The user's library
//! (`library.sqlite`, crate `pv-library`) keeps every revision; a project keeps frozen copies
//! of the components it uses ([`ComponentSnapshot`]), so that it recomputes the same way after
//! the library changes. The engines never see components: [`resolve`] rebuilds the engine
//! types ([`crate::MountingSystem`], [`crate::Article`]) from the frozen copies.
//!
//! [`ComponentSpec`] is stored as typed JSON (serde); [`SPEC_VERSION`] is bumped on every
//! breaking change of its shape, and the library migrates stored specs accordingly.

pub mod compat;
pub mod electrical;
pub mod legacy;
pub mod module;
pub mod mounting;
pub mod part;
pub mod presets;
pub mod price;
pub mod resolve;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::command::{keys, CommandError};
use crate::ids::{BlobId, ComponentId};

pub use compat::check_array_components;
pub use electrical::{
    BatteryConfiguration, BatteryCoupling, BatteryPort, BatteryPowerRule, BatterySpec,
    EquipmentDetails, EquipmentKind, EquipmentSpec, InverterAc, InverterDc, InverterKind,
    InverterSpec, OptimizerSpec, OptimizerStringRule, Property, PropertyValue, Tracker,
};
pub use module::{ModuleDetails, ModuleElectrical, ModuleKind, ModuleSpec};
pub use mounting::{
    Approval, MountingStructure, MountingSystemSpec, PartRuleRef, SystemCompatibility,
    SystemStatics,
};
pub use part::{FixationCapacity, LengthRange, PartKind, PartSpec};
pub use price::{select_price, Currency, IsoDate, PriceEntry};
pub use resolve::{check_system, resolve_mounting, to_article, ComponentLookup};

/// Version of the JSON shape of [`ComponentSpec`].
pub const SPEC_VERSION: u32 = 1;

/// Where a component's data comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ComponentSource {
    /// Entered by hand.
    Manual,
    /// Imported from a manufacturer's list (K2 xlsx, CSV).
    Import,
    /// Extracted from a datasheet by the copilot; never `verified` until reviewed.
    AiExtracted,
}

/// Translated descriptions; a missing language falls back to the manufacturer's label.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Descriptions {
    #[specta(optional)]
    pub fr: Option<String>,
    #[specta(optional)]
    pub de: Option<String>,
    #[specta(optional)]
    pub en: Option<String>,
}

/// Family of a component (the `ComponentSpec` tag, for search filters and error messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ComponentFamily {
    Module,
    MountingSystem,
    MountingPart,
    Inverter,
    Optimizer,
    Battery,
    Equipment,
}

impl ComponentFamily {
    /// The camelCase tag, as stored and as used in i18n `select` parameters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::MountingSystem => "mountingSystem",
            Self::MountingPart => "mountingPart",
            Self::Inverter => "inverter",
            Self::Optimizer => "optimizer",
            Self::Battery => "battery",
            Self::Equipment => "equipment",
        }
    }
}

/// Family-specific data of a component (closed set).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "family")]
pub enum ComponentSpec {
    Module(ModuleSpec),
    MountingSystem(Box<MountingSystemSpec>),
    MountingPart(PartSpec),
    Inverter(Box<InverterSpec>),
    Optimizer(Box<OptimizerSpec>),
    Battery(Box<BatterySpec>),
    Equipment(Box<EquipmentSpec>),
}

impl ComponentSpec {
    #[must_use]
    pub const fn family(&self) -> ComponentFamily {
        match self {
            Self::Module(_) => ComponentFamily::Module,
            Self::MountingSystem(_) => ComponentFamily::MountingSystem,
            Self::MountingPart(_) => ComponentFamily::MountingPart,
            Self::Inverter(_) => ComponentFamily::Inverter,
            Self::Optimizer(_) => ComponentFamily::Optimizer,
            Self::Battery(_) => ComponentFamily::Battery,
            Self::Equipment(_) => ComponentFamily::Equipment,
        }
    }
}

/// One revision of a catalogue entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Stable across revisions.
    pub id: ComponentId,
    /// 1 for the first revision, +1 on every change.
    pub revision: u32,
    pub manufacturer: String,
    /// Manufacturer's model or product name (e.g. `LR7-54HVH-490M`, `SingleRail 36`).
    pub model: String,
    /// Manufacturer's article number; empty when the component has none (modules often).
    pub article_no: String,
    /// Label shown in lists and reports when no translation exists.
    pub label: String,
    pub descriptions: Descriptions,
    pub source: ComponentSource,
    /// Data reviewed by a person (always `false` right after an AI extraction).
    pub verified: bool,
    #[specta(optional)]
    pub datasheet: Option<BlobId>,
    pub spec: ComponentSpec,
}

impl Component {
    /// Description in `lang` (`fr`, `de`, `en`), or the label.
    #[must_use]
    pub fn description(&self, lang: &str) -> &str {
        let d = &self.descriptions;
        match lang {
            "fr" => d.fr.as_deref(),
            "de" => d.de.as_deref(),
            "en" => d.en.as_deref(),
            _ => None,
        }
        .unwrap_or(&self.label)
    }

    /// Checks the component's own values (not its references to other components: see
    /// [`resolve::resolve_mounting`]).
    ///
    /// # Errors
    /// [`keys::COMPONENT_INVALID`] naming the first invalid field.
    pub fn validate(&self) -> Result<(), CommandError> {
        let field = if self.revision == 0 {
            Some("revision")
        } else if self.label.trim().is_empty() {
            Some("label")
        } else if self.manufacturer.trim().is_empty() {
            Some("manufacturer")
        } else {
            match &self.spec {
                ComponentSpec::Module(m) => m.invalid_field(),
                ComponentSpec::MountingSystem(s) => s.invalid_field(),
                ComponentSpec::MountingPart(p) => p.invalid_field(),
                ComponentSpec::Inverter(i) => i.invalid_field(),
                ComponentSpec::Optimizer(o) => o.invalid_field(),
                ComponentSpec::Battery(b) => b.invalid_field(),
                ComponentSpec::Equipment(e) => e.invalid_field(),
            }
        };
        field.map_or(Ok(()), |f| {
            Err(self.error(keys::COMPONENT_INVALID).with_text("field", f))
        })
    }

    /// An error about this component, with its label as `component` parameter.
    #[must_use]
    pub fn error(&self, key: &str) -> CommandError {
        CommandError::new(key).with_text("component", self.label.clone())
    }
}

/// A component frozen into a project, with the price it was adopted at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSnapshot {
    pub component: Component,
    /// `None` when the library had no valid price at adoption.
    #[specta(optional)]
    pub price: Option<PriceEntry>,
}

/// `true` for a finite, strictly positive value.
pub(crate) fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// `true` for a finite value ≥ 0.
pub(crate) fn non_negative(x: f64) -> bool {
    x.is_finite() && x >= 0.0
}
