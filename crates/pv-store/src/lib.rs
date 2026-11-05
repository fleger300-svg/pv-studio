//! PV Studio persistence. One SQLite file (`.pvproj`) per project.
//!
//! No dependency on Tauri: the desktop shell wraps [`ProjectStore`] in managed state.

pub mod buffer;
mod error;
mod schema;
mod store;
#[cfg(test)]
mod test_support;

pub use error::StoreError;
pub use store::{Author, ProjectStore};
