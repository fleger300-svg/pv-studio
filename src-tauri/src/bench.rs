//! Benchmark support: platform facts and report persistence.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pv_model::command::keys;
use pv_model::{CommandError, StressPreset};
use pv_store::report::report_file_name;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// `windows`, `macos`, `linux`.
    pub os: String,
    pub arch: String,
    /// Whether the Rust side is a debug build (benchmarks should use release builds).
    pub debug_build: bool,
}

/// Facts about the host that the WebView cannot report reliably.
#[tauri::command]
#[specta::specta]
pub async fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        debug_build: cfg!(debug_assertions),
    }
}

fn failure(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string())
}

/// Validates and writes a benchmark report (JSON). Dev builds write to the repository's
/// `testdata/`, release builds to `<app data>/benchmarks/`. Returns the file path.
#[tauri::command]
#[specta::specta]
pub async fn write_bench_report(
    app: AppHandle,
    preset: StressPreset,
    report: String,
) -> Result<String, CommandError> {
    let value: serde_json::Value =
        serde_json::from_str(&report).map_err(|_| CommandError::new(keys::INVALID_REPORT))?;
    let dir = if cfg!(debug_assertions) {
        // The workspace root is the parent of `src-tauri`.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.parent().unwrap_or(&manifest).join("testdata")
    } else {
        app.path()
            .app_data_dir()
            .map_err(failure)?
            .join("benchmarks")
    };
    std::fs::create_dir_all(&dir).map_err(failure)?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_secs();
    let path = dir.join(report_file_name(
        &format!("{preset:?}"),
        std::env::consts::OS,
        secs,
    ));
    let pretty = serde_json::to_string_pretty(&value).map_err(failure)? + "\n";
    std::fs::write(&path, pretty).map_err(failure)?;
    Ok(path.display().to_string())
}
