//! Compute thread ↔ front end: events and the results snapshot.
//!
//! The thread itself lives in `pv-pipeline` (tested there); this module only turns its
//! events into Tauri events and keeps the latest results for `get_statics_snapshot`.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

use pv_model::{ArrayId, CommandError};
use pv_pipeline::{ComputeEvent, ComputeHandle, FieldReport};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::state::AppState;

/// Statics of fields that changed (architecture: « Chaque résultat arrive par événement
/// result:ready, avec son empreinte d'entrées »).
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "result:ready")]
pub struct ResultReady {
    pub seq: u32,
    pub fields: Vec<FieldReport>,
    /// Fields deleted from the project.
    pub removed: Vec<ArrayId>,
    /// Every previous result is obsolete (project opened, recovery).
    pub reset: bool,
}

/// The compute thread starts (`busy`) or finishes a pass.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "compute:state")]
pub struct ComputeState {
    pub busy: bool,
    pub seq: u32,
}

/// A pass could not run.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "compute:failed")]
pub struct ComputeFailed {
    pub seq: u32,
    pub error: CommandError,
}

/// Latest results, mirrored from the events.
#[derive(Debug, Clone, Default)]
pub struct ResultCache {
    pub busy: bool,
    pub seq: u32,
    pub fields: BTreeMap<ArrayId, FieldReport>,
}

/// What the front end loads when it (re)attaches to a project.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StaticsSnapshot {
    pub busy: bool,
    pub seq: u32,
    pub fields: Vec<FieldReport>,
}

/// Starts the compute thread; its events update `cache` and go to the front end.
///
/// # Errors
/// When the OS cannot create the thread.
pub fn start(app: &AppHandle, cache: Arc<RwLock<ResultCache>>) -> std::io::Result<ComputeHandle> {
    let app = app.clone();
    ComputeHandle::spawn(move |event| {
        // Notifications are best-effort: a closed window must not stop the thread.
        let mut c = cache.write().unwrap_or_else(PoisonError::into_inner);
        match event {
            ComputeEvent::State { busy, seq } => {
                c.busy = busy;
                c.seq = seq;
                drop(c);
                let _ = ComputeState { busy, seq }.emit(&app);
            }
            ComputeEvent::Ready {
                seq,
                fields,
                removed,
                reset,
            } => {
                if reset {
                    c.fields.clear();
                }
                for a in &removed {
                    c.fields.remove(a);
                }
                for f in &fields {
                    c.fields.insert(f.array_id, f.clone());
                }
                drop(c);
                let _ = ResultReady {
                    seq,
                    fields,
                    removed,
                    reset,
                }
                .emit(&app);
            }
            ComputeEvent::Failed { seq, error } => {
                drop(c);
                let _ = ComputeFailed { seq, error }.emit(&app);
            }
        }
    })
}

/// Latest statics results of the open project (no computation: read from the cache).
#[tauri::command]
#[specta::specta]
pub async fn get_statics_snapshot(
    state: State<'_, AppState>,
) -> Result<StaticsSnapshot, CommandError> {
    let c = state.results.read().unwrap_or_else(PoisonError::into_inner);
    Ok(StaticsSnapshot {
        busy: c.busy,
        seq: c.seq,
        fields: c.fields.values().cloned().collect(),
    })
}
