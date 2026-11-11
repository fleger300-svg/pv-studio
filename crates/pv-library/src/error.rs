use pv_model::command::keys;
use pv_model::CommandError;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    /// Invalid data (validation error with an i18n key).
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialisation error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the library was written by a newer version (schema {found}, supported {supported})")]
    NewerSchema { found: i64, supported: i64 },
    #[error("corrupt library: {0}")]
    Corrupt(String),
}

impl From<LibraryError> for CommandError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::Command(c) => c,
            LibraryError::NewerSchema { found, supported } =>
            {
                #[allow(clippy::cast_precision_loss)]
                CommandError::new(keys::STORE_NEWER_SCHEMA)
                    .with_number("found", found as f64)
                    .with_number("supported", supported as f64)
            }
            other => CommandError::new(keys::STORE_FAILURE).with_text("detail", other.to_string()),
        }
    }
}
