//! Commands: the only way to change a [`Project`]. Every command is serialisable, validated
//! atomically (nothing changes on error) and has an inverse computed *before* it is applied.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ids::ModuleId;
use crate::pricing::ManufacturerDiscount;
use crate::project::{ModuleState, Project};

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
    ];
}

/// Exact position of one module, used by [`Command::SetModulePositions`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct ModulePosition {
    pub id: ModuleId,
    pub u: f64,
    pub v: f64,
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
}

/// What a command changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    /// Sorted, unique.
    pub module_ids: Vec<ModuleId>,
    /// The project's pricing (discounts) changed.
    pub pricing: bool,
}

impl ChangeSet {
    /// Module changes only.
    #[must_use]
    pub fn modules(module_ids: Vec<ModuleId>) -> Self {
        Self {
            module_ids,
            pricing: false,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.module_ids.is_empty() && !self.pricing
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

fn unique_sorted(ids: &[ModuleId]) -> Vec<ModuleId> {
    let mut v = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

fn check_known(project: &Project, ids: &[ModuleId]) -> Result<(), CommandError> {
    let unknown = ids
        .iter()
        .filter(|id| project.module(**id).is_none())
        .count();
    if unknown > 0 {
        return Err(CommandError::new(keys::UNKNOWN_MODULE).with_number("count", unknown as f64));
    }
    Ok(())
}

/// Validates that every `(id, u, v)` keeps the module inside its roof face.
fn check_positions(
    project: &Project,
    targets: &[(ModuleId, f64, f64)],
) -> Result<(), CommandError> {
    let mut outside = 0usize;
    for &(id, u, v) in targets {
        if !u.is_finite() || !v.is_finite() {
            return Err(CommandError::new(keys::INVALID_NUMBER));
        }
        let m = &project.modules[id.index()];
        let (array, face) = project
            .module_context(m)
            .ok_or_else(|| CommandError::new(keys::UNKNOWN_MODULE).with_number("count", 1.0))?;
        if !Project::fits_on_roof(array, face, u, v) {
            outside += 1;
        }
    }
    if outside > 0 {
        return Err(CommandError::new(keys::OUT_OF_ROOF).with_number("count", outside as f64));
    }
    Ok(())
}

fn set_state(
    project: &mut Project,
    ids: &[ModuleId],
    from: ModuleState,
    to: ModuleState,
) -> ChangeSet {
    let mut changed = Vec::new();
    for &id in ids {
        let m = &mut project.modules[id.index()];
        if m.state == from {
            m.state = to;
            changed.push(id);
        }
    }
    ChangeSet::modules(changed)
}

impl Command {
    /// The command that undoes `self`, computed against the state *before* `apply`.
    #[must_use]
    pub fn inverse(&self, project: &Project) -> Command {
        let known = |ids: &[ModuleId]| -> Vec<ModuleId> {
            unique_sorted(ids)
                .into_iter()
                .filter(|id| project.module(*id).is_some())
                .collect()
        };
        let snapshot = |ids: Vec<ModuleId>| Command::SetModulePositions {
            positions: ids
                .into_iter()
                .map(|id| {
                    let m = &project.modules[id.index()];
                    ModulePosition { id, u: m.u, v: m.v }
                })
                .collect(),
        };
        let with_state = |ids: &[ModuleId], state: ModuleState| -> Vec<ModuleId> {
            known(ids)
                .into_iter()
                .filter(|id| project.modules[id.index()].state == state)
                .collect()
        };
        match self {
            Command::MoveModules { ids, .. } => snapshot(known(ids)),
            Command::SetModulePositions { positions } => {
                let ids: Vec<ModuleId> = positions.iter().map(|p| p.id).collect();
                snapshot(known(&ids))
            }
            Command::RemoveModules { ids } => Command::RestoreModules {
                ids: with_state(ids, ModuleState::Active),
            },
            Command::RestoreModules { ids } => Command::RemoveModules {
                ids: with_state(ids, ModuleState::Removed),
            },
            Command::SetDiscount { manufacturer, .. } => Command::SetDiscount {
                manufacturer: manufacturer.clone(),
                rate: project
                    .pricing
                    .discounts
                    .iter()
                    .find(|d| &d.manufacturer == manufacturer)
                    .map(|d| d.rate),
            },
        }
    }

    /// Validates and applies the command. On error the project is left untouched.
    ///
    /// # Errors
    /// A [`CommandError`] with an i18n key (see [`keys`]).
    pub fn apply(&self, project: &mut Project) -> Result<ChangeSet, CommandError> {
        match self {
            Command::MoveModules { ids, du, dv } => {
                if !du.is_finite() || !dv.is_finite() {
                    return Err(CommandError::new(keys::INVALID_NUMBER));
                }
                let ids = unique_sorted(ids);
                check_known(project, &ids)?;
                let targets: Vec<(ModuleId, f64, f64)> = ids
                    .iter()
                    .map(|&id| {
                        let m = &project.modules[id.index()];
                        (id, m.u + du, m.v + dv)
                    })
                    .collect();
                check_positions(project, &targets)?;
                for &(id, u, v) in &targets {
                    let m = &mut project.modules[id.index()];
                    m.u = u;
                    m.v = v;
                }
                Ok(ChangeSet::modules(ids))
            }
            Command::SetModulePositions { positions } => {
                let ids: Vec<ModuleId> = positions.iter().map(|p| p.id).collect();
                if unique_sorted(&ids).len() != ids.len() {
                    return Err(CommandError::new(keys::MISMATCHED_LENGTHS));
                }
                check_known(project, &ids)?;
                let targets: Vec<(ModuleId, f64, f64)> =
                    positions.iter().map(|p| (p.id, p.u, p.v)).collect();
                check_positions(project, &targets)?;
                for p in positions {
                    let m = &mut project.modules[p.id.index()];
                    m.u = p.u;
                    m.v = p.v;
                }
                Ok(ChangeSet::modules(unique_sorted(&ids)))
            }
            Command::RemoveModules { ids } => {
                let ids = unique_sorted(ids);
                check_known(project, &ids)?;
                Ok(set_state(
                    project,
                    &ids,
                    ModuleState::Active,
                    ModuleState::Removed,
                ))
            }
            Command::RestoreModules { ids } => {
                let ids = unique_sorted(ids);
                check_known(project, &ids)?;
                Ok(set_state(
                    project,
                    &ids,
                    ModuleState::Removed,
                    ModuleState::Active,
                ))
            }
            Command::SetDiscount { manufacturer, rate } => {
                if let Some(r) = rate {
                    if !(r.is_finite() && (0.0..1.0).contains(r)) {
                        return Err(CommandError::new(keys::INVALID_DISCOUNT)
                            .with_text("manufacturer", manufacturer.clone())
                            .with_number("rate", *r));
                    }
                }
                let discounts = &mut project.pricing.discounts;
                let current = discounts
                    .iter()
                    .position(|d| &d.manufacturer == manufacturer);
                let changed = match (current, rate) {
                    (Some(i), Some(r)) if discounts[i].rate == *r => false,
                    (Some(i), Some(r)) => {
                        discounts[i].rate = *r;
                        true
                    }
                    (Some(i), None) => {
                        discounts.remove(i);
                        true
                    }
                    (None, Some(r)) => {
                        discounts.push(ManufacturerDiscount {
                            manufacturer: manufacturer.clone(),
                            rate: *r,
                        });
                        true
                    }
                    (None, None) => false,
                };
                Ok(ChangeSet {
                    module_ids: Vec::new(),
                    pricing: changed,
                })
            }
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
