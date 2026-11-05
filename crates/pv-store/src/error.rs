use pv_model::command::keys;
use pv_model::project::ProjectInvariantError;
use pv_model::CommandError;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A rejected command (validation error with an i18n key).
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialisation error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid project: {0}")]
    Invariant(#[from] ProjectInvariantError),
    #[error("the file already exists: {0}")]
    AlreadyExists(String),
    #[error("the file was written by a newer version (schema {found}, supported {supported})")]
    NewerSchema { found: i64, supported: i64 },
    #[error("corrupt project file: {0}")]
    Corrupt(String),
}

impl From<StoreError> for CommandError {
    /// Maps storage errors to translatable errors (key + params). Technical details travel
    /// as a `detail` parameter; the sentence around them is translated.
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Command(c) => c,
            StoreError::AlreadyExists(path) => {
                CommandError::new(keys::STORE_ALREADY_EXISTS).with_text("path", path)
            }
            StoreError::NewerSchema { found, supported } => {
                CommandError::new(keys::STORE_NEWER_SCHEMA)
                    .with_number("found", found as f64)
                    .with_number("supported", supported as f64)
            }
            other => CommandError::new(keys::STORE_FAILURE).with_text("detail", other.to_string()),
        }
    }
}
