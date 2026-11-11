//! Layout commands for the array panel: a new array's defaults, and the automatic layout
//! of a face as one explicit batch (applied with `apply_command`: one undo).

use pv_layout::Search;
use pv_model::command::keys;
use pv_model::component::{presets, ComponentSpec};
use pv_model::{Command, CommandError, PanelArray, RoofFaceId};
use tauri::State;

use crate::state::AppState;

/// A new array for a roof face: the base catalogue's module (LONGi LR7-54HVH-490M) and
/// system (K2 SingleRail), usual gaps and setbacks. The components are frozen when the array
/// is applied (`apply_layout` / `apply_command`).
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
        let mut array = pv_layout::default_array(face);
        // The module as frozen in the project, else as the library holds it now.
        let module = match store.project().component(presets::LR7_54HVH_490M) {
            Some(s) => Some(s.component.clone()),
            None => state
                .with_library(|lib| {
                    Ok(lib
                        .get(presets::LR7_54HVH_490M)
                        .map_err(CommandError::from)?
                        .map(|e| e.component))
                })
                .ok()
                .flatten(),
        };
        if let Some(ComponentSpec::Module(m)) = module.as_ref().map(|c| &c.spec) {
            array.module_length_m = m.length_m;
            array.module_width_m = m.width_m;
            array.module = Some(presets::LR7_54HVH_490M);
            array.mounting = Some(presets::K2_SINGLE_RAIL);
        }
        Ok(array)
    })
}

/// The layout batch of `array`, preceded by the adoption of the library components it refers
/// to that the project lacks (the planner validates the array against the project, so they
/// are frozen on a copy first). One batch: one undo step.
fn layout_command(
    state: &AppState,
    project: &pv_model::Project,
    array: &PanelArray,
) -> Result<Command, CommandError> {
    let completed = state.complete_command(
        project,
        Command::UpsertArray {
            array: Box::new(array.clone()),
            at: None,
        },
    )?;
    let Command::Batch { mut commands } = completed else {
        return pv_layout::plan_layout(project, array, Search::default());
    };
    let adopt = commands.remove(0);
    let mut preview = project.clone();
    adopt.apply(&mut preview)?;
    let layout = pv_layout::plan_layout(&preview, array, Search::default())?;
    Ok(Command::Batch {
        commands: vec![adopt, layout],
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
    state.with(|store| layout_command(&state, store.project(), &array))
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
        let cmd = layout_command(&state, store.project(), &array)?;
        let changes = store
            .apply(&cmd, pv_store::Author::User)
            .map_err(CommandError::from)?;
        crate::commands::finish(&app, &state, store, changes, started)
    })
}
