//! Strongly typed identifiers. Entity ids are UUIDv7 (time-ordered); module ids are dense
//! `u32` indices that double as GPU instance indices.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Creates a new time-ordered (UUIDv7) id.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

uuid_id!(
    /// Identifies a project.
    ProjectId
);
uuid_id!(
    /// Identifies a building.
    BuildingId
);
uuid_id!(
    /// Identifies a roof face.
    RoofFaceId
);
uuid_id!(
    /// Identifies a panel array.
    ArrayId
);
uuid_id!(
    /// Identifies a library component across all its revisions.
    ComponentId
);
uuid_id!(
    /// Identifies a stored file (datasheet PDF) of the component library.
    BlobId
);

/// Dense module id. Equal to the module's index in [`crate::Project::modules`], to the
/// `module_instance.id` primary key, and to the global GPU instance index.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct ModuleId(pub u32);

impl ModuleId {
    /// Index into `Project::modules`.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}
