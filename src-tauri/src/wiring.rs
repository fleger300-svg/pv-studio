//! DC design: the checks of the open project's wiring and the automatic stringing.

use std::time::Instant;

use pv_electrical::WiringReport;
use pv_model::{Command, CommandError};
use pv_store::Author;
use tauri::{AppHandle, State};

use crate::commands::{finish, ApplyResult};
use crate::state::AppState;

/// Checks of the open project's inverters and strings (computed on demand: fast).
#[tauri::command]
#[specta::specta]
pub async fn wiring_report(state: State<'_, AppState>) -> Result<WiringReport, CommandError> {
    state.with(|store| Ok(pv_electrical::check(store.project())))
}

/// Rebuilds every unlocked string of the open project automatically. One undo step.
#[tauri::command]
#[specta::specta]
pub async fn wiring_auto(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = Instant::now();
        let wiring = pv_electrical::auto::auto_string(store.project());
        let changes = store
            .apply(
                &Command::SetWiring {
                    wiring: Box::new(wiring),
                },
                Author::User,
            )
            .map_err(CommandError::from)?;
        finish(&app, &state, store, changes, started)
    })
}
