//! Tauri managed state: the currently open project and the compute thread's link.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock};

use pv_library::LibraryStore;
use pv_model::command::keys;
use pv_model::{ChangeSet, CommandError};
use pv_pipeline::ComputeHandle;
use pv_store::ProjectStore;

use crate::compute::ResultCache;

/// Holds at most one open project. `rusqlite::Connection` is `Send` but not `Sync`,
/// hence a `Mutex` rather than an `RwLock`.
#[derive(Default)]
pub struct AppState {
    project: Mutex<Option<ProjectStore>>,
    /// Sequence number of the last command (or project open) sent to the compute thread.
    seq: AtomicU32,
    /// Started in `setup()` (the event sink needs the `AppHandle`).
    compute: OnceLock<ComputeHandle>,
    /// Latest statics results, for the snapshot command (written by the compute thread).
    pub results: Arc<RwLock<ResultCache>>,
    /// The user's component library (opened at start-up). Lock order: project, then
    /// library.
    library: Mutex<Option<LibraryStore>>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Option<ProjectStore>> {
        // A panic while holding the lock cannot leave the store half-written (SQLite
        // transactions), so recovering the guard is safe.
        self.project.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Attaches the compute thread (once, at start-up).
    pub fn attach_compute(&self, handle: ComputeHandle) {
        let _ = self.compute.set(handle);
    }

    fn next_seq(&self) -> u32 {
        self.seq.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Sequence number of the last command sent to the compute thread.
    pub fn current_seq(&self) -> u32 {
        self.seq.load(Ordering::Acquire)
    }

    /// Closes the open project, if any (releases the SQLite file).
    pub fn clear(&self) {
        *self.lock() = None;
    }

    /// Replaces the open project (the previous one is closed); the compute thread rebuilds
    /// its graph from it.
    pub fn set(&self, store: ProjectStore) {
        let mut guard = self.lock();
        if let Some(c) = self.compute.get() {
            c.link().open(self.next_seq(), store.project());
        }
        *guard = Some(store);
    }

    /// After a successful command (called with the store lock held, so that batches reach
    /// the compute thread in command order).
    pub fn submit(&self, store: &ProjectStore, changes: &ChangeSet) {
        if changes.is_empty() {
            return;
        }
        if let Some(c) = self.compute.get() {
            c.link().submit(self.next_seq(), store.project(), changes);
        }
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

    /// Installs the component library.
    pub fn set_library(&self, library: LibraryStore) {
        *self.library.lock().unwrap_or_else(PoisonError::into_inner) = Some(library);
    }

    /// Runs `f` on the component library.
    ///
    /// # Errors
    /// `errors.store.failure` when the library could not be opened, or whatever `f` returns.
    pub fn with_library<T>(
        &self,
        f: impl FnOnce(&mut LibraryStore) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let mut guard = self.library.lock().unwrap_or_else(PoisonError::into_inner);
        let library = guard.as_mut().ok_or_else(|| {
            CommandError::new(keys::STORE_FAILURE).with_text("detail", "component library")
        })?;
        f(library)
    }

    /// `cmd` with the adoption of the library components it needs (see
    /// `pv_library::adoption`); unchanged when it needs none or no library is open.
    ///
    /// # Errors
    /// A component missing from the library.
    pub fn complete_command(
        &self,
        project: &pv_model::Project,
        cmd: pv_model::Command,
    ) -> Result<pv_model::Command, CommandError> {
        if pv_library::adoption::referenced_components(&cmd)
            .iter()
            .all(|&id| project.component(id).is_some())
        {
            return Ok(cmd);
        }
        self.with_library(|lib| {
            pv_library::adoption::complete_command(
                lib,
                project,
                cmd,
                &pv_library::adoption::today(),
            )
            .map_err(CommandError::from)
        })
    }
}
