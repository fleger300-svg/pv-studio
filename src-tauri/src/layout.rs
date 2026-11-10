//! Layout commands for the array panel: a new array's defaults, and the automatic layout
//! of a face as one explicit batch (applied with `apply_command`: one undo).

use pv_layout::Search;
use pv_model::command::keys;
use pv_model::{Command, CommandError, PanelArray, RoofFaceId};
use tauri::State;

use crate::state::AppState;

/// A new array for a roof face (reference module, K2 SingleRail, usual gaps and setbacks).
#[tauri::command]
#[specta::specta]
pub async fn default_array(
    state: State<'_, AppState>,
    face_id: RoofFaceId,
) -> Result<PanelArray, CommandError> {
    state.with(|store| {
        let face = store
            .project()
            .roof_face(face_id)
            .ok_or_else(|| CommandError::new(keys::ENTITY_UNKNOWN).with_text("kind", "roofFace"))?;
        Ok(pv_layout::default_array(face))
    })
}

/// The batch that gives `array` (new or changed) its settings and lays out its whole face
/// again. Pure: nothing changes until the front-end applies it.
#[tauri::command]
#[specta::specta]
pub async fn plan_layout(
    state: State<'_, AppState>,
    array: PanelArray,
) -> Result<Command, CommandError> {
    state.with(|store| pv_layout::plan_layout(store.project(), &array, Search::default()))
}

/// Plans and applies the layout of `array`'s face in one call (the journal records the
/// explicit batch, like `apply_command`): large fields do not send their thousands of
/// module slots to the front-end and back.
#[tauri::command]
#[specta::specta]
pub async fn apply_layout(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    array: PanelArray,
) -> Result<crate::commands::ApplyResult, CommandError> {
    state.with(|store| {
        let started = std::time::Instant::now();
        let cmd = pv_layout::plan_layout(store.project(), &array, Search::default())?;
        let changes = store
            .apply(&cmd, pv_store::Author::User)
            .map_err(CommandError::from)?;
        crate::commands::finish(&app, &state, store, changes, started)
    })
}
