//! Tauri commands and events. Thin adapters: all logic lives in `pv-model` / `pv-store`.

use std::path::PathBuf;
use std::time::Instant;

use pv_model::command::keys;
use pv_model::{ChangeSet, Command, CommandError, ModuleId, ProjectSummary, StressPreset};
use pv_store::{Author, ProjectStore};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::{InvokeResponseBody, IpcResponse, Response};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::state::AppState;

/// Raw `PVS1` bytes (see `pv_store::buffer`), sent as a binary IPC response
/// (`tauri::ipc::Response`), never as JSON. The front-end receives an `ArrayBuffer`.
pub struct ModuleBuffer(Response);

impl ModuleBuffer {
    fn new(bytes: Vec<u8>) -> Self {
        Self(Response::new(bytes))
    }
}

impl IpcResponse for ModuleBuffer {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        self.0.body()
    }
}

impl Type for ModuleBuffer {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta::datatype::DataType::Reference(specta_typescript::define("ArrayBuffer"))
    }
}

fn summary(store: &ProjectStore) -> Result<ProjectSummary, CommandError> {
    let (can_undo, can_redo) = store.history_state().map_err(CommandError::from)?;
    Ok(ProjectSummary::new(
        store.project(),
        store.path().display().to_string(),
        can_undo,
        can_redo,
    ))
}

/// Default location of a stress project file (in the app data directory).
#[tauri::command]
#[specta::specta]
pub async fn stress_project_path(
    app: AppHandle,
    preset: StressPreset,
) -> Result<String, CommandError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string()))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string()))?;
    let name = match preset {
        StressPreset::Residential1k => "stress-residential-1k.pvproj",
        StressPreset::Industrial10k => "stress-industrial-10k.pvproj",
    };
    Ok(dir.join(name).display().to_string())
}

/// Builds a synthetic project into a new `.pvproj` file and opens it. Stress projects are
/// disposable: an existing file at `path` (and its WAL/SHM side files) is replaced, so every
/// benchmark run starts from the same state.
#[tauri::command]
#[specta::specta]
pub async fn create_stress_project(
    state: State<'_, AppState>,
    preset: StressPreset,
    path: String,
) -> Result<ProjectSummary, CommandError> {
    let path = PathBuf::from(path);
    // Close the open project first: Windows cannot delete a file that is still open.
    state.clear();
    for suffix in ["", "-wal", "-shm"] {
        let mut file = path.clone().into_os_string();
        file.push(suffix);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(
                    CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string())
                )
            }
        }
    }
    let store = ProjectStore::create(&path, preset.build())?;
    let result = summary(&store)?;
    state.set(store);
    Ok(result)
}

/// Opens an existing `.pvproj` file (applying migrations) and makes it the open project.
#[tauri::command]
#[specta::specta]
pub async fn open_project(
    state: State<'_, AppState>,
    path: String,
) -> Result<ProjectSummary, CommandError> {
    let store = ProjectStore::open(&PathBuf::from(path))?;
    let result = summary(&store)?;
    state.set(store);
    Ok(result)
}

/// Binary buffer of every module of the open project.
#[tauri::command]
#[specta::specta]
pub async fn get_module_buffer(state: State<'_, AppState>) -> Result<ModuleBuffer, CommandError> {
    state.with(|store| Ok(ModuleBuffer::new(store.module_buffer())))
}

/// Binary buffer of the given modules only (used to resync after undo/redo).
#[tauri::command]
#[specta::specta]
pub async fn get_module_buffer_for(
    state: State<'_, AppState>,
    ids: Vec<ModuleId>,
) -> Result<ModuleBuffer, CommandError> {
    state.with(|store| Ok(ModuleBuffer::new(store.module_buffer_for(&ids))))
}

/// Result of `apply_command`, `undo` and `redo`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub changes: ChangeSet,
    /// Time spent in Rust (validation + apply + journal), microseconds.
    pub rust_us: f64,
    pub can_undo: bool,
    pub can_redo: bool,
    pub active_module_count: u32,
}

/// Emitted after every applied command, undo or redo.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "model:changed")]
pub struct ModelChanged {
    pub module_ids: Vec<ModuleId>,
    pub can_undo: bool,
    pub can_redo: bool,
}

fn finish(
    app: &AppHandle,
    store: &ProjectStore,
    changes: ChangeSet,
    started: Instant,
) -> Result<ApplyResult, CommandError> {
    let rust_us = started.elapsed().as_secs_f64() * 1e6;
    let (can_undo, can_redo) = store.history_state().map_err(CommandError::from)?;
    let event = ModelChanged {
        module_ids: changes.module_ids.clone(),
        can_undo,
        can_redo,
    };
    // The command succeeded; a failed notification must not turn it into an error.
    let _ = event.emit(app);
    Ok(ApplyResult {
        changes,
        rust_us,
        can_undo,
        can_redo,
        active_module_count: store.project().active_count() as u32,
    })
}

/// Validates, applies and journals a model command.
#[tauri::command]
#[specta::specta]
pub async fn apply_command(
    app: AppHandle,
    state: State<'_, AppState>,
    cmd: Command,
) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = Instant::now();
        let changes = store
            .apply(&cmd, Author::User)
            .map_err(CommandError::from)?;
        finish(&app, store, changes, started)
    })
}

/// Undoes the last command.
#[tauri::command]
#[specta::specta]
pub async fn undo(app: AppHandle, state: State<'_, AppState>) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = Instant::now();
        let changes = store.undo().map_err(CommandError::from)?;
        finish(&app, store, changes, started)
    })
}

/// Redoes the last undone command.
#[tauri::command]
#[specta::specta]
pub async fn redo(app: AppHandle, state: State<'_, AppState>) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = Instant::now();
        let changes = store.redo().map_err(CommandError::from)?;
        finish(&app, store, changes, started)
    })
}
