//! Compute thread ↔ front end: events and the results snapshot.
//!
//! The thread itself lives in `pv-pipeline` (tested there); this module only turns its
//! events into Tauri events and keeps the latest results for `get_statics_snapshot`.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

use pv_model::command::keys;
use pv_model::{ArrayId, CommandError};
use pv_pipeline::{BomOutcome, BomReport, ComputeEvent, ComputeHandle, FieldReport};
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
    /// The project's bill of materials, when it changed.
    pub bom: Option<BomReport>,
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
    pub bom: Option<BomReport>,
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
                bom,
                removed,
                reset,
            } => {
                if reset {
                    c.fields.clear();
                    c.bom = None;
                }
                if let Some(b) = &bom {
                    c.bom = Some(b.clone());
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
                    bom,
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

/// Latest bill of materials of the open project (from the cache; `None` before the first
/// pass).
#[tauri::command]
#[specta::specta]
pub async fn get_bom(state: State<'_, AppState>) -> Result<Option<BomReport>, CommandError> {
    let c = state.results.read().unwrap_or_else(PoisonError::into_inner);
    Ok(c.bom.clone())
}

/// A frozen bill of materials.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BomSnapshotInfo {
    pub id: u32,
    pub created_at: String,
    pub label: String,
    /// Command the bill of materials is valid for.
    pub seq: u32,
    pub input_fingerprint: String,
}

impl From<pv_store::BomSnapshotRecord> for BomSnapshotInfo {
    fn from(r: pv_store::BomSnapshotRecord) -> Self {
        Self {
            id: u32::try_from(r.id).unwrap_or(u32::MAX),
            created_at: r.created_at,
            label: r.label,
            seq: r.seq,
            input_fingerprint: r.input_fingerprint,
        }
    }
}

/// How long `freeze_bom` waits for the compute thread to reach the last command (the worst
/// case measured is ~30 ms on 10 000 modules).
const FREEZE_WAIT: Duration = Duration::from_secs(2);

/// Freezes the bill of materials of the current project state (the basis of a quote or an
/// export). Waits until the compute thread has caught up with the last command; refuses a
/// stale or failed bill of materials.
#[tauri::command]
#[specta::specta]
pub async fn freeze_bom(
    state: State<'_, AppState>,
    label: String,
) -> Result<BomSnapshotInfo, CommandError> {
    let target = state.current_seq();
    let deadline = Instant::now() + FREEZE_WAIT;
    let report = loop {
        {
            let c = state.results.read().unwrap_or_else(PoisonError::into_inner);
            if !c.busy && c.seq >= target {
                if let Some(b) = &c.bom {
                    break b.clone();
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(CommandError::new(keys::BOM_STALE));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let bom = match &report.outcome {
        BomOutcome::Ok { bom } => bom,
        BomOutcome::Failed { error } => return Err(error.clone()),
    };
    let json = serde_json::to_string(bom)
        .map_err(|e| CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string()))?;
    state.with(|store| {
        store
            .save_bom_snapshot(&label, target, &report.input_fingerprint, &json)
            .map(BomSnapshotInfo::from)
            .map_err(CommandError::from)
    })
}

/// Frozen bills of materials of the open project, oldest first.
#[tauri::command]
#[specta::specta]
pub async fn list_bom_snapshots(
    state: State<'_, AppState>,
) -> Result<Vec<BomSnapshotInfo>, CommandError> {
    state.with(|store| {
        Ok(store
            .bom_snapshots()
            .map_err(CommandError::from)?
            .into_iter()
            .map(BomSnapshotInfo::from)
            .collect())
    })
}

/// A frozen bill of materials, read back for an export.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FrozenBom {
    pub info: BomSnapshotInfo,
    pub bom: pv_bom::Bom,
    pub pricing: pv_model::Pricing,
}

/// One frozen bill of materials, exactly as it was recorded.
#[tauri::command]
#[specta::specta]
pub async fn get_bom_snapshot(
    state: State<'_, AppState>,
    id: u32,
) -> Result<FrozenBom, CommandError> {
    state.with(|store| {
        let info = store
            .bom_snapshots()
            .map_err(CommandError::from)?
            .into_iter()
            .find(|s| s.id == i64::from(id))
            .ok_or_else(|| CommandError::new(keys::BOM_STALE))?;
        let (bom, pricing) = store
            .bom_snapshot_json(i64::from(id))
            .map_err(CommandError::from)?
            .ok_or_else(|| CommandError::new(keys::BOM_STALE))?;
        let parse = |e: serde_json::Error| {
            CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string())
        };
        Ok(FrozenBom {
            info: info.into(),
            bom: serde_json::from_str(&bom).map_err(parse)?,
            pricing: serde_json::from_str(&pricing).map_err(parse)?,
        })
    })
}
