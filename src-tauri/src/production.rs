//! Yield simulation of the open project (hourly over its weather year; on demand).

use pv_model::command::keys;
use pv_model::CommandError;
use pv_solar::module_shading::{module_shading as shading_of, ShadingMap};
use pv_solar::roof_shading::{roof_shading as roof_map, RoofShadingMap};
use pv_solar::shade::SurroundingsView;
use pv_solar::{simulate, NotSimulated, SimulationAssumptions, YieldReport};
use tauri::State;

use crate::state::AppState;

fn not_simulated(e: NotSimulated) -> CommandError {
    match e {
        NotSimulated::NoWeather => CommandError::new(keys::YIELD_NO_WEATHER),
        NotSimulated::NoModules => CommandError::new(keys::YIELD_NO_MODULES),
    }
}

/// Most cells a side of the surroundings drawn in the 3-D view.
const SURROUNDINGS_MAX_SIDE: usize = 400;

/// The surroundings of the open project (terrain, neighbouring buildings of the surface
/// model) as they shade it, reduced for the 3-D view; `None` without any.
#[tauri::command]
#[specta::specta]
pub async fn surroundings_view(
    state: State<'_, AppState>,
) -> Result<Option<SurroundingsView>, CommandError> {
    let project = state.with(|store| Ok(store.project().clone()))?;
    tauri::async_runtime::spawn_blocking(move || {
        pv_solar::shade::surroundings_view(&project, SURROUNDINGS_MAX_SIDE)
    })
    .await
    .map_err(|e| CommandError::new(keys::STATICS_INTERNAL).with_text("detail", e.to_string()))
}

/// Shading map of the roof faces of the open project over its weather year (share of the
/// irradiation of a surface lying on the roof lost to the horizon and the near shading, on
/// a grid over each face), for the 3-D view.
#[tauri::command]
#[specta::specta]
pub async fn roof_shading(state: State<'_, AppState>) -> Result<RoofShadingMap, CommandError> {
    let project = state.with(|store| Ok(store.project().clone()))?;
    tauri::async_runtime::spawn_blocking(move || {
        roof_map(&project, SimulationAssumptions::default().albedo).map_err(not_simulated)
    })
    .await
    .map_err(|e| CommandError::new(keys::STATICS_INTERNAL).with_text("detail", e.to_string()))?
}

/// Shading of each module of the open project over its weather year (share of its
/// irradiation lost to the horizon and to the near shading), for the 3-D view.
#[tauri::command]
#[specta::specta]
pub async fn module_shading(state: State<'_, AppState>) -> Result<ShadingMap, CommandError> {
    let project = state.with(|store| Ok(store.project().clone()))?;
    tauri::async_runtime::spawn_blocking(move || {
        shading_of(&project, SimulationAssumptions::default().albedo).map_err(not_simulated)
    })
    .await
    .map_err(|e| CommandError::new(keys::STATICS_INTERNAL).with_text("detail", e.to_string()))?
}

/// Simulates the AC yield of the open project.
#[tauri::command]
#[specta::specta]
pub async fn simulate_yield(state: State<'_, AppState>) -> Result<YieldReport, CommandError> {
    let project = state.with(|store| Ok(store.project().clone()))?;
    // A year of hourly steps over a few hundred modules: well under a second, off the
    // store lock.
    tauri::async_runtime::spawn_blocking(move || {
        simulate(&project, &SimulationAssumptions::default()).map_err(not_simulated)
    })
    .await
    .map_err(|e| CommandError::new(keys::STATICS_INTERNAL).with_text("detail", e.to_string()))?
}
