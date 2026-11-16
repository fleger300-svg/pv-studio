//! Import of PV*SOL premium projects (`.pvprj`) as new PV Studio projects.

use std::path::{Path, PathBuf};

use pv_model::command::keys;
use pv_model::{CommandError, ProjectSummary};
use pv_pvsol::{Comparison, ImportNote};
use pv_store::ProjectStore;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::state::AppState;

/// What the import produced: the new (open) project, what PV*SOL had that was not carried
/// over, and PV Studio's simulation next to PV*SOL's results.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PvsolImport {
    pub summary: ProjectSummary,
    /// File name of the PV*SOL project.
    pub source: String,
    pub notes: Vec<ImportNote>,
    /// `None` when PV*SOL has no results or the project cannot be simulated.
    #[specta(optional)]
    pub comparison: Option<Comparison>,
}

fn failure(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string())
}

fn read_error(e: &pv_pvsol::ReadError) -> CommandError {
    CommandError::new(e.key()).with_text("detail", e.to_string())
}

/// A file name from a project title (characters Windows refuses replaced).
fn file_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    format!("{}.pvproj", if clean.is_empty() { "Projet" } else { clean })
}

/// Imports a PV*SOL project into a new `.pvproj` and opens it. Without `source` / `target`,
/// asks with the native « Ouvrir » / « Enregistrer sous » dialogs (`path`s for automation);
/// `None` when the user cancels. An existing file is never replaced.
#[tauri::command]
#[specta::specta]
pub async fn import_pvsol(
    app: AppHandle,
    state: State<'_, AppState>,
    source: Option<String>,
    target: Option<String>,
) -> Result<Option<PvsolImport>, CommandError> {
    let source = match source {
        Some(p) => PathBuf::from(p),
        None => {
            let chosen = app
                .dialog()
                .file()
                .add_filter("PV*SOL", &["pvprj"])
                .blocking_pick_file();
            match chosen {
                Some(file) => file.into_path().map_err(failure)?,
                None => return Ok(None),
            }
        }
    };
    let bytes = std::fs::read(&source).map_err(failure)?;
    let file = pv_pvsol::read(&bytes).map_err(|e| read_error(&e))?;
    let stem = source
        .file_stem()
        .map_or_else(|| "Projet".to_owned(), |s| s.to_string_lossy().into_owned());
    let outcome = pv_pvsol::import(&file, &stem)?;

    let target = match target {
        Some(p) => PathBuf::from(p),
        None => {
            let mut dialog = app
                .dialog()
                .file()
                .set_file_name(file_name(&outcome.project.name))
                .add_filter("PV Studio", &["pvproj"]);
            if let Some(dir) = source.parent() {
                dialog = dialog.set_directory(dir);
            }
            match dialog.blocking_save_file() {
                Some(file) => file.into_path().map_err(failure)?,
                None => return Ok(None),
            }
        }
    };
    let comparison = pv_pvsol::compare(&file, &outcome.project);
    let store = ProjectStore::create(&target, outcome.project)?;
    let summary = crate::commands::summary(&store)?;
    state.set(store);
    Ok(Some(PvsolImport {
        summary,
        source: source_name(&source),
        notes: outcome.notes,
        comparison,
    }))
}

fn source_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("Projet A"), "Projet A.pvproj");
        assert_eq!(file_name("A/B: C?"), "A_B_ C_.pvproj");
        assert_eq!(file_name("  "), "Projet.pvproj");
    }
}
