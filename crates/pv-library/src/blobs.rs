//! Datasheets and other files, deduplicated by SHA-256.

use pv_model::BlobId;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{LibraryError, LibraryStore};

/// A stored file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl LibraryStore {
    /// Stores `bytes` (or finds the identical file already stored) and returns its id.
    ///
    /// # Errors
    /// A database failure.
    pub fn put_blob(&mut self, bytes: &[u8], media_type: &str) -> Result<BlobId, LibraryError> {
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let existing: Option<String> = self
            .conn
            .query_row("SELECT id FROM blob WHERE sha256 = ?1", [&hash], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(id) = existing {
            let uuid = Uuid::parse_str(&id).map_err(|e| LibraryError::Corrupt(e.to_string()))?;
            return Ok(BlobId(uuid));
        }
        let id = BlobId::new();
        self.conn.execute(
            "INSERT INTO blob (id, sha256, media_type, size, bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.to_string(),
                hash,
                media_type,
                i64::try_from(bytes.len()).unwrap_or(i64::MAX),
                bytes
            ],
        )?;
        Ok(id)
    }

    /// # Errors
    /// A database failure.
    pub fn blob(&self, id: BlobId) -> Result<Option<Blob>, LibraryError> {
        Ok(self
            .conn
            .query_row(
                "SELECT media_type, bytes FROM blob WHERE id = ?1",
                [id.to_string()],
                |r| {
                    Ok(Blob {
                        media_type: r.get(0)?,
                        bytes: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }
}
