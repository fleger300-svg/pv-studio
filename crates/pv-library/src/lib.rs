//! PV Studio component library: the user's catalogue of modules, mounting systems and parts
//! (`library.sqlite`, one per user, in the application data folder).
//!
//! - Every change of a component is a new revision; all revisions are kept (audit). Components
//!   are archived, never deleted (projects may refer to them).
//! - Prices are list prices per piece in integer cents, per supplier, with validity periods.
//! - Projects never read the library during computations: they freeze the components they use
//!   ([`pv_model::ComponentSnapshot`], see [`snapshot`]) and get update proposals.
//! - Library changes are not commands of a project and never enter a project journal.

pub mod adoption;
mod blobs;
mod components;
mod error;
pub mod import;
mod prices;
pub mod schema;
mod search;
pub mod seed;
pub mod snapshot;

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

pub use blobs::Blob;
pub use components::{LibraryEntry, RevisionInfo};
pub use error::LibraryError;
pub use search::{ComponentSummary, SearchQuery};

/// Who changed the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChangeAuthor {
    User,
    Import,
    Seed,
    Copilot,
}

impl ChangeAuthor {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Import => "import",
            Self::Seed => "seed",
            Self::Copilot => "copilot",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "user" => Self::User,
            "import" => Self::Import,
            "seed" => Self::Seed,
            "copilot" => Self::Copilot,
            _ => return None,
        })
    }
}

/// An open component library.
pub struct LibraryStore {
    conn: Connection,
}

impl LibraryStore {
    /// Opens (or creates) the library at `path`, migrates it and adds the missing base
    /// catalogue components.
    ///
    /// # Errors
    /// A newer schema, or a database failure.
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        Self::prepared(Connection::open(path)?)
    }

    /// In-memory library (tests, previews).
    ///
    /// # Errors
    /// A database failure.
    pub fn open_in_memory() -> Result<Self, LibraryError> {
        Self::prepared(Connection::open_in_memory()?)
    }

    fn prepared(mut conn: Connection) -> Result<Self, LibraryError> {
        schema::prepare(&mut conn)?;
        let mut store = Self { conn };
        store.seed()?;
        Ok(store)
    }
}
