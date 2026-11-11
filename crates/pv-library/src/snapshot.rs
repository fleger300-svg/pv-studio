//! Freezing components into a project (the closure of what a field uses, with the prices
//! valid on a date), and update proposals: what changed in the library since a component was
//! frozen, field by field. Nothing is applied until the user accepts a proposal (an undoable
//! `UpdateComponents` command).

use std::collections::BTreeMap;

use pv_model::component::{ComponentSpec, IsoDate, PriceEntry};
use pv_model::{Command, ComponentId, ComponentSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::{LibraryError, LibraryStore};

/// One changed field (`path` in the component's JSON, values as JSON text).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub path: String,
    #[specta(optional)]
    pub before: Option<String>,
    #[specta(optional)]
    pub after: Option<String>,
}

/// A newer library version of a frozen component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProposal {
    pub component: ComponentId,
    pub label: String,
    pub from_revision: u32,
    pub to_revision: u32,
    /// Data changes (revision excluded); empty when only the price changed.
    pub changes: Vec<FieldChange>,
    #[specta(optional)]
    pub price_before: Option<PriceEntry>,
    #[specta(optional)]
    pub price_after: Option<PriceEntry>,
    /// The snapshot that accepting the proposal freezes.
    pub proposed: ComponentSnapshot,
}

impl UpdateProposal {
    #[must_use]
    pub fn price_changed(&self) -> bool {
        self.price_before != self.price_after
    }
}

/// `path → value` of every leaf of `v`.
fn flatten(v: &Value, path: &str, out: &mut BTreeMap<String, String>) {
    match v {
        Value::Object(map) => {
            for (k, x) in map {
                flatten(x, &format!("{path}.{k}"), out);
            }
        }
        Value::Array(items) => {
            for (i, x) in items.iter().enumerate() {
                flatten(x, &format!("{path}[{i}]"), out);
            }
            if items.is_empty() {
                out.insert(path.to_owned(), "[]".to_owned());
            }
        }
        leaf => {
            out.insert(path.to_owned(), leaf.to_string());
        }
    }
}

/// Field changes from `before` to `after` (both serialisable), `revision` excluded.
///
/// # Errors
/// Serialisation failure.
pub fn field_changes<T: Serialize>(
    before: &T,
    after: &T,
) -> Result<Vec<FieldChange>, LibraryError> {
    let (mut a, mut b) = (BTreeMap::new(), BTreeMap::new());
    flatten(&serde_json::to_value(before)?, "", &mut a);
    flatten(&serde_json::to_value(after)?, "", &mut b);
    let mut paths: Vec<&String> = a.keys().chain(b.keys()).collect();
    paths.sort();
    paths.dedup();
    Ok(paths
        .into_iter()
        .filter(|p| p.as_str() != ".revision" && a.get(*p) != b.get(*p))
        .map(|p| FieldChange {
            path: p.trim_start_matches('.').to_owned(),
            before: a.get(p).cloned(),
            after: b.get(p).cloned(),
        })
        .collect())
}

impl LibraryStore {
    /// The components to freeze for `ids` (modules and/or systems): each of them, plus every
    /// part the systems refer to, without duplicates, parts before their systems, each with
    /// the price valid on `date`.
    ///
    /// # Errors
    /// A missing component, or a database failure.
    pub fn closure(
        &self,
        ids: &[ComponentId],
        date: &IsoDate,
    ) -> Result<Vec<ComponentSnapshot>, LibraryError> {
        let mut order: Vec<ComponentId> = Vec::new();
        for c in self.load_all(ids)? {
            if let ComponentSpec::MountingSystem(spec) = &c.spec {
                for part in spec.part_ids() {
                    if !order.contains(&part) {
                        order.push(part);
                    }
                }
            }
            if !order.contains(&c.id) {
                order.push(c.id);
            }
        }
        self.load_all(&order)?
            .into_iter()
            .map(|component| {
                let price = self.price_on(component.id, date)?;
                Ok(ComponentSnapshot { component, price })
            })
            .collect()
    }

    /// Proposals for the frozen `components` whose library version (data or price valid on
    /// `date`) differs. Components unknown to the library (converted from old files, or from
    /// another user's library) get none.
    ///
    /// # Errors
    /// A database failure.
    pub fn update_proposals(
        &self,
        components: &[ComponentSnapshot],
        date: &IsoDate,
    ) -> Result<Vec<UpdateProposal>, LibraryError> {
        let mut out = Vec::new();
        for frozen in components {
            let id = frozen.component.id;
            let Some(entry) = self.get(id)? else {
                continue;
            };
            let current = entry.component;
            if current.revision < frozen.component.revision {
                continue;
            }
            let price = self.price_on(id, date)?;
            // A price that only differs by its period is the same price.
            let same_price = match (&frozen.price, &price) {
                (Some(a), Some(b)) => {
                    a.list_price_cents == b.list_price_cents
                        && a.currency == b.currency
                        && a.supplier == b.supplier
                }
                (None, None) => true,
                _ => false,
            };
            let changes = if current == frozen.component {
                Vec::new()
            } else {
                field_changes(&frozen.component, &current)?
            };
            if changes.is_empty() && same_price && current.revision == frozen.component.revision {
                continue;
            }
            out.push(UpdateProposal {
                component: id,
                label: current.label.clone(),
                from_revision: frozen.component.revision,
                to_revision: current.revision,
                changes,
                price_before: frozen.price.clone(),
                price_after: if same_price {
                    frozen.price.clone()
                } else {
                    price.clone()
                },
                proposed: ComponentSnapshot {
                    component: current,
                    price: if same_price {
                        frozen.price.clone()
                    } else {
                        price
                    },
                },
            });
        }
        Ok(out)
    }
}

/// The command accepting `proposals` (one undo step).
#[must_use]
pub fn accept_command(proposals: &[UpdateProposal]) -> Command {
    Command::UpdateComponents {
        snapshots: proposals.iter().map(|p| p.proposed.clone()).collect(),
    }
}
