//! Writing exported files (Excel bills of materials) chosen by the user.
//!
//! The front end builds the file in a Web Worker; Rust only asks where to save it (native
//! « Enregistrer sous » dialog) and writes the bytes.

use std::path::PathBuf;

use pv_model::command::keys;
use pv_model::CommandError;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

fn failure(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string())
}

/// Saves `bytes` as an Excel file. Without `path`, asks the user where (native dialog,
/// `suggested_name` pre-filled); `None` when the user cancels. With `path` (automation,
/// e.g. tests), writes there without asking.
#[tauri::command]
#[specta::specta]
pub async fn save_export(
    app: AppHandle,
    suggested_name: String,
    bytes: Vec<u8>,
    path: Option<String>,
) -> Result<Option<String>, CommandError> {
    let target: PathBuf = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let chosen = app
                .dialog()
                .file()
                .set_file_name(&suggested_name)
                .add_filter("Excel", &["xlsx"])
                .blocking_save_file();
            match chosen {
                Some(file) => file.into_path().map_err(failure)?,
                None => return Ok(None),
            }
        }
    };
    std::fs::write(&target, bytes).map_err(failure)?;
    Ok(Some(target.display().to_string()))
}
