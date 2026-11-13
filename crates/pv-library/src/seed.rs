//! Base catalogue ([`pv_model::component::presets`]), added to every library on open.
//!
//! Only missing components are added: a base component the user has edited (or archived)
//! keeps the user's revision. Base components carry no prices.

use pv_model::component::presets;

use crate::{ChangeAuthor, LibraryError, LibraryStore};

impl LibraryStore {
    /// Adds the base components missing from the library; returns how many were added.
    ///
    /// # Errors
    /// A database failure, or an invalid preset (a bug: presets are tested).
    pub fn seed(&mut self) -> Result<usize, LibraryError> {
        let mut added = 0;
        // Parts come before the systems that refer to them.
        for component in presets::all() {
            if self.get(component.id)?.is_none() {
                self.create(component, ChangeAuthor::Seed, "base catalogue")?;
                added += 1;
            }
        }
        // Datasheet catalogue: unverified until the user reviews each entry.
        // Systems last: they refer to parts that must exist first.
        let mut catalogue = crate::catalog::components()?;
        catalogue.sort_by_key(|c| matches!(c.spec, pv_model::ComponentSpec::MountingSystem(_)));
        for component in catalogue {
            if self.get(component.id)?.is_none() {
                self.create(component, ChangeAuthor::Import, "datasheet catalogue")?;
                added += 1;
            }
        }
        Ok(added)
    }
}
