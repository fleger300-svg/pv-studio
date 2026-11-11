//! Components: create, update (new revision), archive, read, history.

use pv_model::command::keys;
use pv_model::component::{check_system, ComponentSnapshot, ComponentSpec, SPEC_VERSION};
use pv_model::{CommandError, Component, ComponentId};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ChangeAuthor, LibraryError, LibraryStore};

/// A component as the library holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub component: Component,
    pub archived: bool,
}

/// One past revision of a component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RevisionInfo {
    pub revision: u32,
    pub changed_at: String,
    pub author: ChangeAuthor,
    pub note: String,
}

fn parse_component(json: &str) -> Result<Component, LibraryError> {
    Ok(serde_json::from_str(json)?)
}

impl LibraryStore {
    /// Current revision of `id`.
    ///
    /// # Errors
    /// A database failure or unreadable data.
    pub fn get(&self, id: ComponentId) -> Result<Option<LibraryEntry>, LibraryError> {
        let row: Option<(String, bool)> = self
            .conn
            .query_row(
                "SELECT data, archived FROM component WHERE id = ?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(data, archived)| {
            Ok(LibraryEntry {
                component: parse_component(&data)?,
                archived,
            })
        })
        .transpose()
    }

    /// The component `id` as it was at `revision`.
    ///
    /// # Errors
    /// A database failure or unreadable data.
    pub fn get_revision(
        &self,
        id: ComponentId,
        revision: u32,
    ) -> Result<Option<Component>, LibraryError> {
        let data: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM component_revision WHERE component_id = ?1 AND revision = ?2",
                params![id.to_string(), revision],
                |r| r.get(0),
            )
            .optional()?;
        data.as_deref().map(parse_component).transpose()
    }

    /// History of `id`, oldest first.
    ///
    /// # Errors
    /// A database failure.
    pub fn revisions(&self, id: ComponentId) -> Result<Vec<RevisionInfo>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT revision, changed_at, author, note FROM component_revision
              WHERE component_id = ?1 ORDER BY revision",
        )?;
        let rows = stmt.query_map([id.to_string()], |r| {
            let author: String = r.get(2)?;
            Ok(RevisionInfo {
                revision: r.get(0)?,
                changed_at: r.get(1)?,
                author: ChangeAuthor::parse(&author).unwrap_or(ChangeAuthor::User),
                note: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Several components (current revisions), as snapshots without price, in `ids` order.
    ///
    /// # Errors
    /// [`keys::COMPONENT_UNKNOWN`] for a missing id.
    pub fn load_all(&self, ids: &[ComponentId]) -> Result<Vec<Component>, LibraryError> {
        ids.iter()
            .map(|&id| {
                self.get(id)?.map(|e| e.component).ok_or_else(|| {
                    CommandError::new(keys::COMPONENT_UNKNOWN)
                        .with_text("id", id.to_string())
                        .into()
                })
            })
            .collect()
    }

    /// Checks a component before storing it: its own values and, for a system, every part it
    /// refers to (they must already be in the library).
    fn check(&self, component: &Component) -> Result<(), LibraryError> {
        component.validate()?;
        if let ComponentSpec::MountingSystem(spec) = &component.spec {
            let parts: Vec<ComponentSnapshot> = self
                .load_all(&spec.part_ids())?
                .into_iter()
                .map(|component| ComponentSnapshot {
                    component,
                    price: None,
                })
                .collect();
            check_system(component, parts.as_slice())?;
        }
        Ok(())
    }

    /// Adds a new component (stored as revision 1, whatever `component.revision` says).
    ///
    /// # Errors
    /// Invalid data, an id already in the library ([`keys::COMPONENT_EXISTS`]), or a
    /// database failure.
    pub fn create(
        &mut self,
        mut component: Component,
        author: ChangeAuthor,
        note: &str,
    ) -> Result<Component, LibraryError> {
        component.revision = 1;
        guard_verified(&mut component, author);
        self.check(&component)?;
        if self.get(component.id)?.is_some() {
            return Err(CommandError::new(keys::COMPONENT_EXISTS)
                .with_text("id", component.id.to_string())
                .into());
        }
        let tx = self.conn.transaction()?;
        let data = serde_json::to_string(&component)?;
        tx.execute(
            "INSERT INTO component (id, revision, spec_version, data) VALUES (?1, 1, ?2, ?3)",
            params![component.id.to_string(), SPEC_VERSION, data],
        )?;
        insert_revision(&tx, &component, &data, author, note)?;
        tx.commit()?;
        Ok(component)
    }

    /// Stores `component` as a new revision. `component.revision` must be the revision it was
    /// edited from (optimistic concurrency); unchanged data stores nothing.
    ///
    /// # Errors
    /// Invalid data, an unknown id, a stale revision ([`keys::COMPONENT_STALE_REVISION`]), or
    /// a database failure.
    pub fn update(
        &mut self,
        mut component: Component,
        author: ChangeAuthor,
        note: &str,
    ) -> Result<Component, LibraryError> {
        let current = self.get(component.id)?.ok_or_else(|| {
            CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", component.id.to_string())
        })?;
        let found = current.component.revision;
        if component.revision != found {
            return Err(component
                .error(keys::COMPONENT_STALE_REVISION)
                .with_number("expected", f64::from(component.revision))
                .with_number("found", f64::from(found))
                .into());
        }
        if component == current.component {
            return Ok(component);
        }
        component.revision = found + 1;
        guard_verified(&mut component, author);
        self.check(&component)?;
        let tx = self.conn.transaction()?;
        let data = serde_json::to_string(&component)?;
        tx.execute(
            "UPDATE component SET revision = ?2, spec_version = ?3, data = ?4,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id = ?1",
            params![
                component.id.to_string(),
                component.revision,
                SPEC_VERSION,
                data
            ],
        )?;
        insert_revision(&tx, &component, &data, author, note)?;
        tx.commit()?;
        Ok(component)
    }

    /// Archives (hides from search) or restores a component. Archived components stay
    /// readable: projects may refer to them.
    ///
    /// # Errors
    /// An unknown id or a database failure.
    pub fn set_archived(&mut self, id: ComponentId, archived: bool) -> Result<(), LibraryError> {
        let n = self.conn.execute(
            "UPDATE component SET archived = ?2 WHERE id = ?1",
            params![id.to_string(), archived],
        )?;
        if n == 0 {
            return Err(CommandError::new(keys::COMPONENT_UNKNOWN)
                .with_text("id", id.to_string())
                .into());
        }
        Ok(())
    }
}

fn insert_revision(
    tx: &Transaction<'_>,
    c: &Component,
    data: &str,
    author: ChangeAuthor,
    note: &str,
) -> Result<(), LibraryError> {
    tx.execute(
        "INSERT INTO component_revision (component_id, revision, spec_version, data, author, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            c.id.to_string(),
            c.revision,
            SPEC_VERSION,
            data,
            author.as_str(),
            note
        ],
    )?;
    Ok(())
}

/// Only a person verifies data: the copilot's revisions are never `verified` (a datasheet read
/// by AI stays unverified until the user reviews it and saves a revision of their own).
fn guard_verified(component: &mut Component, author: ChangeAuthor) {
    if author == ChangeAuthor::Copilot {
        component.verified = false;
    }
}
