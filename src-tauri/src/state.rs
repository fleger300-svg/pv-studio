//! Tauri managed state: the currently open project.

use std::sync::{Mutex, MutexGuard, PoisonError};

use pv_model::command::keys;
use pv_model::CommandError;
use pv_store::ProjectStore;

/// Holds at most one open project. `rusqlite::Connection` is `Send` but not `Sync`,
/// hence a `Mutex` rather than an `RwLock`.
#[derive(Default)]
pub struct AppState {
    project: Mutex<Option<ProjectStore>>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Option<ProjectStore>> {
        // A panic while holding the lock cannot leave the store half-written (SQLite
        // transactions), so recovering the guard is safe.
        self.project.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Closes the open project, if any (releases the SQLite file).
    pub fn clear(&self) {
        *self.lock() = None;
    }

    /// Replaces the open project (the previous one is closed).
    pub fn set(&self, store: ProjectStore) {
        *self.lock() = Some(store);
    }

    /// Runs `f` on the open project.
    ///
    /// # Errors
    /// `errors.project.notOpen`, or whatever `f` returns.
    pub fn with<T>(
        &self,
        f: impl FnOnce(&mut ProjectStore) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let mut guard = self.lock();
        let store = guard
            .as_mut()
            .ok_or_else(|| CommandError::new(keys::PROJECT_NOT_OPEN))?;
        f(store)
    }
}
