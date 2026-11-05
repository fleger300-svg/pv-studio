//! PV Studio desktop shell. Only this crate depends on Tauri; business logic lives in
//! `crates/pv-model` and `crates/pv-store`.

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run the PV Studio application");
}
