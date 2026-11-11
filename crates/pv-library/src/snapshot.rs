//! Freezing components into a project: the closure of what a field uses, with the prices
//! valid on the project date.

use pv_model::component::{ComponentSpec, IsoDate};
use pv_model::{ComponentId, ComponentSnapshot};

use crate::{LibraryError, LibraryStore};

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
}
