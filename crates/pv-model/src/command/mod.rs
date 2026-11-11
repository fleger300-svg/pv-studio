//! Commands: the only way to change a [`Project`]. Every command is serialisable, validated
//! atomically (nothing changes on error) and has an inverse computed *before* it is applied.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ids::{ArrayId, BuildingId, ModuleId, RoofFaceId};
use crate::project::{Building, ModuleState, PanelArray, Project, RoofFace};

mod batch;
mod entities;
mod modules;
mod pricing;
mod structure;

/// i18n keys of every error the back-end can return. The front-end translates them; never
/// show raw keys. A test checks that each key exists in `packages/locales/en.json`.
pub mod keys {
    pub const OUT_OF_ROOF: &str = "errors.module.outOfRoof";
    pub const UNKNOWN_MODULE: &str = "errors.module.unknown";
    pub const INVALID_NUMBER: &str = "errors.command.invalidNumber";
    pub const INVALID_DISCOUNT: &str = "errors.command.invalidDiscount";
    pub const MISMATCHED_LENGTHS: &str = "errors.command.mismatchedLengths";
    pub const NOTHING_TO_UNDO: &str = "errors.history.nothingToUndo";
    pub const NOTHING_TO_REDO: &str = "errors.history.nothingToRedo";
    pub const PROJECT_NOT_OPEN: &str = "errors.project.notOpen";
    pub const STORE_ALREADY_EXISTS: &str = "errors.store.alreadyExists";
    pub const STORE_NEWER_SCHEMA: &str = "errors.store.newerSchema";
    pub const STORE_FAILURE: &str = "errors.store.failure";
    pub const INVALID_REPORT: &str = "errors.bench.invalidReport";
    /// Layout validation (pv-layout).
    pub const LAYOUT_EDGE_SETBACK: &str = "errors.layout.edgeSetback";
    pub const LAYOUT_OBSTACLE_OVERLAP: &str = "errors.layout.obstacleOverlap";
    pub const LAYOUT_OBSTACLE_CLEARANCE: &str = "errors.layout.obstacleClearance";
    pub const LAYOUT_MODULE_OVERLAP: &str = "errors.layout.moduleOverlap";
    pub const LAYOUT_ARRAY_DISTANCE: &str = "errors.layout.arrayDistance";
    pub const LAYOUT_INVALID_OUTLINE: &str = "errors.layout.invalidOutline";
    pub const LAYOUT_INVALID_PARAMETERS: &str = "errors.layout.invalidParameters";
    /// Statics (pv-pipeline / pv-statics).
    pub const STATICS_SITE_MISSING: &str = "errors.statics.siteMissing";
    pub const STATICS_MOUNTING_MISSING: &str = "errors.statics.mountingMissing";
    pub const STATICS_OUT_OF_RANGE: &str = "errors.statics.outOfRange";
    pub const STATICS_MISSING_COEFFICIENT: &str = "errors.statics.missingCoefficient";
    pub const STATICS_UNSUPPORTED: &str = "errors.statics.unsupported";
    pub const STATICS_NO_SUPPORT_GRID: &str = "errors.statics.noSupportGrid";
    pub const STATICS_EAST_WEST_UNPAIRED: &str = "errors.statics.eastWestUnpaired";
    pub const STATICS_INTERNAL: &str = "errors.statics.internal";
    /// Bill of materials (pv-bom).
    pub const BOM_ZERO_PACK: &str = "errors.bom.zeroPack";
    pub const BOM_INVALID_PIECE: &str = "errors.bom.invalidPiece";
    pub const BOM_INVALID_STOCK: &str = "errors.bom.invalidStock";
    pub const BOM_INVALID_DISCOUNT: &str = "errors.bom.invalidDiscount";
    pub const BOM_STALE: &str = "errors.bom.stale";
    /// Entity commands (buildings, roof faces, arrays).
    pub const ENTITY_UNKNOWN: &str = "errors.entity.unknown";
    pub const ENTITY_REFERENCE: &str = "errors.entity.reference";
    pub const ENTITY_INVALID: &str = "errors.entity.invalid";
    /// Roof generation (pv-roof).
    pub const ROOF_REENTRANT_CORNER: &str = "errors.roof.reentrantCorner";
    /// Component library (pv-model `component`, pv-library).
    pub const COMPONENT_INVALID: &str = "errors.component.invalid";
    pub const COMPONENT_UNKNOWN: &str = "errors.component.unknown";
    pub const COMPONENT_WRONG_FAMILY: &str = "errors.component.wrongFamily";
    pub const COMPONENT_DRIVER_MISMATCH: &str = "errors.component.driverMismatch";
    pub const COMPONENT_STOCK_MISMATCH: &str = "errors.component.stockMismatch";
    pub const COMPONENT_INVALID_DATE: &str = "errors.component.invalidDate";
    pub const COMPAT_FRAME_HEIGHT: &str = "errors.component.frameHeight";
    pub const COMPAT_SUPPORT: &str = "errors.component.support";
    pub const COMPAT_MODULE_SIZE: &str = "errors.component.moduleSize";
    pub const COMPONENT_STALE_REVISION: &str = "errors.component.staleRevision";
    pub const COMPONENT_EXISTS: &str = "errors.component.exists";

    /// All keys, for the locale consistency test.
    pub const ALL: &[&str] = &[
        OUT_OF_ROOF,
        UNKNOWN_MODULE,
        INVALID_NUMBER,
        INVALID_DISCOUNT,
        MISMATCHED_LENGTHS,
        NOTHING_TO_UNDO,
        NOTHING_TO_REDO,
        PROJECT_NOT_OPEN,
        STORE_ALREADY_EXISTS,
        STORE_NEWER_SCHEMA,
        STORE_FAILURE,
        INVALID_REPORT,
        LAYOUT_EDGE_SETBACK,
        LAYOUT_OBSTACLE_OVERLAP,
        LAYOUT_OBSTACLE_CLEARANCE,
        LAYOUT_MODULE_OVERLAP,
        LAYOUT_ARRAY_DISTANCE,
        LAYOUT_INVALID_OUTLINE,
        LAYOUT_INVALID_PARAMETERS,
        STATICS_SITE_MISSING,
        STATICS_MOUNTING_MISSING,
        STATICS_OUT_OF_RANGE,
        STATICS_MISSING_COEFFICIENT,
        STATICS_UNSUPPORTED,
        STATICS_NO_SUPPORT_GRID,
        STATICS_EAST_WEST_UNPAIRED,
        STATICS_INTERNAL,
        BOM_ZERO_PACK,
        BOM_INVALID_PIECE,
        BOM_INVALID_STOCK,
        BOM_INVALID_DISCOUNT,
        BOM_STALE,
        ENTITY_UNKNOWN,
        ENTITY_REFERENCE,
        ENTITY_INVALID,
        ROOF_REENTRANT_CORNER,
        COMPONENT_INVALID,
        COMPONENT_UNKNOWN,
        COMPONENT_WRONG_FAMILY,
        COMPONENT_DRIVER_MISMATCH,
        COMPONENT_STOCK_MISMATCH,
        COMPONENT_INVALID_DATE,
        COMPAT_FRAME_HEIGHT,
        COMPAT_SUPPORT,
        COMPAT_MODULE_SIZE,
        COMPONENT_STALE_REVISION,
        COMPONENT_EXISTS,
    ];
}

/// Exact position of one module, used by [`Command::SetModulePositions`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct ModulePosition {
    pub id: ModuleId,
    pub u: f64,
    pub v: f64,
}

/// One module of an array, without its id (ids are assigned canonically, see
/// [`Project::compact_modules`]). Used by [`Command::SetArrayModules`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModuleSlot {
    pub row: u32,
    pub col: u32,
    pub u: f64,
    pub v: f64,
    pub state: ModuleState,
    pub string_index: u32,
}

/// A serialisable model change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum Command {
    /// Translates modules in their roof's `(u, v)` frame, metres.
    MoveModules {
        ids: Vec<ModuleId>,
        du: f64,
        dv: f64,
    },
    /// Marks modules as removed (they keep their slot and GPU instance index).
    RemoveModules { ids: Vec<ModuleId> },
    /// Marks removed modules as active again.
    RestoreModules { ids: Vec<ModuleId> },
    /// Sets exact module positions. Used as the inverse of `MoveModules` so that undo
    /// restores the original coordinates bit for bit (`(u + du) - du != u` in floating point).
    SetModulePositions { positions: Vec<ModulePosition> },
    /// Sets (`Some`) or removes (`None`) the discount of a manufacturer on the whole project.
    SetDiscount {
        manufacturer: String,
        rate: Option<f64>,
    },
    /// Replaces the building with the same id, or inserts it at `at` (clamped; `None`: last).
    UpsertBuilding {
        building: Box<Building>,
        at: Option<u32>,
    },
    /// Removes a building with its roof faces, arrays and modules.
    RemoveBuilding { id: BuildingId },
    /// Replaces the roof face with the same id, or inserts it at `at`.
    UpsertRoofFace {
        face: Box<RoofFace>,
        at: Option<u32>,
    },
    /// Removes a roof face with its arrays and modules.
    RemoveRoofFace { id: RoofFaceId },
    /// Replaces the array with the same id (its modules are kept), or inserts it at `at`.
    UpsertArray {
        array: Box<PanelArray>,
        at: Option<u32>,
    },
    /// Removes an array with its modules; arrays paired with it are unpaired.
    RemoveArray { id: ArrayId },
    /// Replaces all modules of an array. Structural: module ids are renumbered.
    SetArrayModules {
        array_id: ArrayId,
        modules: Vec<ModuleSlot>,
    },
    /// Several commands applied as one, all or nothing.
    Batch { commands: Vec<Command> },
}

/// What a command changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    /// Modules whose position or state changed. Sorted, unique; empty when `structure`.
    pub module_ids: Vec<ModuleId>,
    /// The project's pricing (discounts) changed.
    pub pricing: bool,
    /// Buildings added, changed or removed. Sorted, unique.
    pub buildings: Vec<BuildingId>,
    /// Roof faces added, changed or removed. Sorted, unique.
    pub faces: Vec<RoofFaceId>,
    /// Arrays added, changed or removed. Sorted, unique.
    pub arrays: Vec<ArrayId>,
    /// Modules were added or removed for good: ids were renumbered, every module-indexed
    /// view (GPU buffers, selections) must be rebuilt.
    pub structure: bool,
}

fn union<T: Ord + Copy>(a: &mut Vec<T>, b: &[T]) {
    a.extend_from_slice(b);
    a.sort_unstable();
    a.dedup();
}

impl ChangeSet {
    /// Module changes only.
    #[must_use]
    pub fn modules(module_ids: Vec<ModuleId>) -> Self {
        Self {
            module_ids,
            ..Self::default()
        }
    }

    /// Whether only module positions / states (and pricing) changed: no entity, no structure.
    #[must_use]
    pub fn modules_only(&self) -> bool {
        !self.structure
            && self.buildings.is_empty()
            && self.faces.is_empty()
            && self.arrays.is_empty()
    }

    /// Accumulates `other` (applied after `self`).
    pub fn merge(&mut self, other: ChangeSet) {
        self.pricing |= other.pricing;
        self.structure |= other.structure;
        union(&mut self.buildings, &other.buildings);
        union(&mut self.faces, &other.faces);
        union(&mut self.arrays, &other.arrays);
        if self.structure {
            // Ids before a renumbering mean nothing after it: everything is reloaded.
            self.module_ids.clear();
        } else {
            union(&mut self.module_ids, &other.module_ids);
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.module_ids.is_empty() && !self.pricing && self.modules_only()
    }
}

/// A parameter of a translated error message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum ErrorParam {
    Number(f64),
    Text(String),
}

/// A command failure, expressed as an i18n key plus ICU parameters — never as prose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, thiserror::Error)]
#[error("{key}")]
pub struct CommandError {
    pub key: String,
    pub params: BTreeMap<String, ErrorParam>,
}

impl CommandError {
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            params: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_number(mut self, name: &str, value: f64) -> Self {
        self.params
            .insert(name.to_owned(), ErrorParam::Number(value));
        self
    }

    #[must_use]
    pub fn with_text(mut self, name: &str, value: impl Into<String>) -> Self {
        self.params
            .insert(name.to_owned(), ErrorParam::Text(value.into()));
        self
    }
}

impl Command {
    /// The command that undoes `self`, computed against the state *before* `apply`.
    #[must_use]
    pub fn inverse(&self, project: &Project) -> Command {
        match self {
            Command::MoveModules { ids, .. } => modules::inverse_move(project, ids),
            Command::SetModulePositions { positions } => {
                modules::inverse_set_positions(project, positions)
            }
            Command::RemoveModules { ids } => modules::inverse_remove(project, ids),
            Command::RestoreModules { ids } => modules::inverse_restore(project, ids),
            Command::SetDiscount { manufacturer, .. } => {
                pricing::inverse_discount(project, manufacturer)
            }
            Command::UpsertBuilding { building, .. } => {
                entities::inverse_upsert_building(project, building)
            }
            Command::RemoveBuilding { id } => entities::inverse_remove_building(project, *id),
            Command::UpsertRoofFace { face, .. } => entities::inverse_upsert_face(project, face),
            Command::RemoveRoofFace { id } => entities::inverse_remove_face(project, *id),
            Command::UpsertArray { array, .. } => entities::inverse_upsert_array(project, array),
            Command::RemoveArray { id } => entities::inverse_remove_array(project, *id),
            Command::SetArrayModules { array_id, .. } => {
                structure::inverse_set_modules(project, *array_id)
            }
            Command::Batch { commands } => batch::inverse_batch(project, commands),
        }
    }

    /// Validates and applies the command. On error the project is left untouched.
    ///
    /// # Errors
    /// A [`CommandError`] with an i18n key (see [`keys`]).
    pub fn apply(&self, project: &mut Project) -> Result<ChangeSet, CommandError> {
        match self {
            Command::MoveModules { ids, du, dv } => modules::apply_move(project, ids, *du, *dv),
            Command::SetModulePositions { positions } => {
                modules::apply_set_positions(project, positions)
            }
            Command::RemoveModules { ids } => {
                modules::apply_state(project, ids, ModuleState::Active, ModuleState::Removed)
            }
            Command::RestoreModules { ids } => {
                modules::apply_state(project, ids, ModuleState::Removed, ModuleState::Active)
            }
            Command::SetDiscount { manufacturer, rate } => {
                pricing::apply_discount(project, manufacturer, *rate)
            }
            Command::UpsertBuilding { building, at } => {
                entities::apply_upsert_building(project, building, *at)
            }
            Command::RemoveBuilding { id } => entities::apply_remove_building(project, *id),
            Command::UpsertRoofFace { face, at } => entities::apply_upsert_face(project, face, *at),
            Command::RemoveRoofFace { id } => entities::apply_remove_face(project, *id),
            Command::UpsertArray { array, at } => entities::apply_upsert_array(project, array, *at),
            Command::RemoveArray { id } => entities::apply_remove_array(project, *id),
            Command::SetArrayModules { array_id, modules } => {
                structure::apply_set_modules(project, *array_id, modules)
            }
            Command::Batch { commands } => batch::apply_batch(project, commands),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::fixtures::small_project;

    fn ids(v: &[u32]) -> Vec<ModuleId> {
        v.iter().map(|&i| ModuleId(i)).collect()
    }

    #[test]
    fn move_then_inverse_restores_exact_state() {
        let mut p = small_project();
        let original = p.clone();
        // Values chosen so that (u + du) - du != u in floating point.
        let cmd = Command::MoveModules {
            ids: ids(&[0, 1, 1, 4]),
            du: 0.1,
            dv: 0.2,
        };
        let inv = cmd.inverse(&p);
        let cs = cmd.apply(&mut p).expect("move applies");
        assert_eq!(cs.module_ids, ids(&[0, 1, 4]), "duplicates moved once");
        assert!((p.modules[0].u - original.modules[0].u - 0.1).abs() < 1e-12);
        inv.apply(&mut p).expect("inverse applies");
        assert_eq!(p, original, "bit-exact original state");
        for (a, b) in p.modules.iter().zip(&original.modules) {
            assert_eq!(a.u.to_bits(), b.u.to_bits());
            assert_eq!(a.v.to_bits(), b.v.to_bits());
        }
    }

    #[test]
    fn out_of_roof_move_is_rejected_with_i18n_key() {
        let mut p = small_project();
        let before = p.clone();
        let cmd = Command::MoveModules {
            ids: ids(&[0, 3, 5]),
            du: 0.0,
            dv: 2.5,
        };
        let err = cmd.apply(&mut p).expect_err("must be rejected");
        assert_eq!(err.key, "errors.module.outOfRoof");
        // Row 1 modules (3, 5) leave the 6 m face; row 0 module (0) still fits.
        assert_eq!(err.params.get("count"), Some(&ErrorParam::Number(2.0)));
        assert_eq!(p, before, "nothing changed on error");
    }

    #[test]
    fn unknown_module_is_rejected() {
        let mut p = small_project();
        let err = Command::RemoveModules { ids: ids(&[2, 99]) }
            .apply(&mut p)
            .expect_err("unknown");
        assert_eq!(err.key, keys::UNKNOWN_MODULE);
        assert!(p.modules.iter().all(|m| m.state == ModuleState::Active));
    }

    #[test]
    fn non_finite_delta_is_rejected() {
        let mut p = small_project();
        let err = Command::MoveModules {
            ids: ids(&[0]),
            du: f64::NAN,
            dv: 0.0,
        }
        .apply(&mut p)
        .expect_err("nan");
        assert_eq!(err.key, keys::INVALID_NUMBER);
    }

    #[test]
    fn remove_restore_round_trip() {
        let mut p = small_project();
        Command::RemoveModules { ids: ids(&[1]) }
            .apply(&mut p)
            .expect("pre-remove");
        let original = p.clone();
        let cmd = Command::RemoveModules {
            ids: ids(&[0, 1, 2]),
        };
        let inv = cmd.inverse(&p);
        assert_eq!(
            inv,
            Command::RestoreModules { ids: ids(&[0, 2]) },
            "only active ones"
        );
        let cs = cmd.apply(&mut p).expect("remove");
        assert_eq!(cs.module_ids, ids(&[0, 2]));
        inv.apply(&mut p).expect("restore");
        assert_eq!(p, original);
    }

    #[test]
    fn every_error_key_is_translated() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/locales/en.json"
        );
        let en: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("read en.json"))
                .expect("json");
        for key in keys::ALL {
            let found = key.split('.').try_fold(&en, |node, part| node.get(part));
            assert!(
                found.is_some_and(serde_json::Value::is_string),
                "missing translation for {key}"
            );
        }
    }

    #[test]
    fn command_json_round_trip() {
        let cmd = Command::MoveModules {
            ids: ids(&[3, 7]),
            du: -0.5,
            dv: 1.25,
        };
        let json = serde_json::to_string(&cmd).expect("serialise");
        assert_eq!(
            json,
            r#"{"type":"MoveModules","ids":[3,7],"du":-0.5,"dv":1.25}"#
        );
        let back: Command = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, cmd);
    }

    fn set_discount(rate: Option<f64>) -> Command {
        Command::SetDiscount {
            manufacturer: "K2 Systems".into(),
            rate,
        }
    }

    #[test]
    fn set_discount_applies_and_undoes_exactly() {
        let mut p = small_project();
        let before = p.clone();
        let inverse = set_discount(Some(0.35)).inverse(&p);
        assert_eq!(inverse, set_discount(None));
        let c = set_discount(Some(0.35)).apply(&mut p).expect("apply");
        assert!(c.pricing && c.module_ids.is_empty());
        assert!((p.pricing.rate_for("K2 Systems") - 0.35).abs() < 1e-12);
        // Changing it: the inverse restores the previous rate.
        assert_eq!(
            set_discount(Some(0.4)).inverse(&p),
            set_discount(Some(0.35))
        );
        // The same rate again changes nothing (the store does not journal it).
        assert!(set_discount(Some(0.35))
            .apply(&mut p)
            .expect("same")
            .is_empty());
        inverse.apply(&mut p).expect("undo");
        assert_eq!(p, before);
        // Removing a discount that does not exist changes nothing.
        assert!(set_discount(None).apply(&mut p).expect("none").is_empty());
    }

    #[test]
    fn set_discount_rejects_invalid_rates() {
        let mut p = small_project();
        for rate in [1.0, -0.1, f64::NAN] {
            let e = set_discount(Some(rate)).apply(&mut p).unwrap_err();
            assert_eq!(e.key, keys::INVALID_DISCOUNT);
        }
        assert!(p.pricing.discounts.is_empty());
    }
}
