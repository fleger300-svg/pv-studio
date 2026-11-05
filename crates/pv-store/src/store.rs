//! Open project + command journal (undo/redo) backed by one SQLite file.

use std::path::{Path, PathBuf};

use pv_model::command::keys;
use pv_model::{
    ArrayGrid, ArrayId, Building, BuildingId, ChangeSet, Command, CommandError, ModuleId,
    ModuleInstance, ModuleState, Orientation, PanelArray, Project, ProjectId, RoofFace, RoofFaceId,
    RoofKind,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use uuid::Uuid;

use crate::{buffer, schema, StoreError};

/// Who issued a command (`command_log.author`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Author {
    User,
    Copilot,
    Import,
}

impl Author {
    fn as_str(self) -> &'static str {
        match self {
            Author::User => "user",
            Author::Copilot => "copilot",
            Author::Import => "import",
        }
    }
}

/// An open `.pvproj` file and its in-memory model.
pub struct ProjectStore {
    conn: Connection,
    project: Project,
    path: PathBuf,
}

fn uuid_col(row: &Row<'_>, idx: usize) -> rusqlite::Result<Uuid> {
    let s: String = row.get(idx)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn enum_col<T>(row: &Row<'_>, idx: usize, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    let s: String = row.get(idx)?;
    parse(&s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            format!("unexpected value {s:?}").into(),
        )
    })
}

fn state_str(s: ModuleState) -> &'static str {
    match s {
        ModuleState::Active => "active",
        ModuleState::Removed => "removed",
    }
}

impl ProjectStore {
    /// Creates a new project file. Fails if `path` already exists.
    ///
    /// # Errors
    /// I/O, SQLite or invariant errors.
    pub fn create(path: &Path, project: Project) -> Result<Self, StoreError> {
        project.validate()?;
        if path.exists() {
            return Err(StoreError::AlreadyExists(path.display().to_string()));
        }
        let mut conn = Connection::open(path)?;
        schema::prepare(&mut conn)?;
        let tx = conn.transaction()?;
        insert_project(&tx, &project)?;
        tx.commit()?;
        Ok(Self {
            conn,
            project,
            path: path.to_owned(),
        })
    }

    /// Opens an existing project file, applying pending migrations.
    ///
    /// # Errors
    /// I/O, SQLite, schema or invariant errors.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if !path.is_file() {
            return Err(StoreError::Corrupt(format!(
                "not a file: {}",
                path.display()
            )));
        }
        let mut conn = Connection::open(path)?;
        schema::prepare(&mut conn)?;
        let project = load_project(&conn)?;
        project.validate()?;
        Ok(Self {
            conn,
            project,
            path: path.to_owned(),
        })
    }

    #[must_use]
    pub fn project(&self) -> &Project {
        &self.project
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Validates, applies and journals a command. A new command truncates the redo branch.
    /// Commands that change nothing are not journalled.
    ///
    /// # Errors
    /// [`StoreError::Command`] when validation fails (model unchanged), or a storage error
    /// (model rolled back).
    pub fn apply(&mut self, cmd: &Command, author: Author) -> Result<ChangeSet, StoreError> {
        let inverse = cmd.inverse(&self.project);
        let changes = cmd.apply(&mut self.project)?;
        if changes.module_ids.is_empty() {
            return Ok(changes);
        }
        let result = (|| -> Result<(), StoreError> {
            let tx = self.conn.transaction()?;
            tx.execute("DELETE FROM command_log WHERE undone = 1", [])?;
            tx.execute(
                "INSERT INTO command_log (author, command, inverse) VALUES (?1, ?2, ?3)",
                params![
                    author.as_str(),
                    serde_json::to_string(cmd)?,
                    serde_json::to_string(&inverse)?
                ],
            )?;
            persist_modules(&tx, &self.project, &changes.module_ids)?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = result {
            // Keep memory and disk consistent. The inverse is valid by construction.
            let _ = inverse.apply(&mut self.project);
            return Err(e);
        }
        Ok(changes)
    }

    /// Undoes the most recent non-undone command.
    ///
    /// # Errors
    /// `errors.history.nothingToUndo`, or storage errors.
    pub fn undo(&mut self) -> Result<ChangeSet, StoreError> {
        let entry: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT seq, inverse FROM command_log WHERE undone = 0 ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (seq, json) = entry.ok_or_else(|| CommandError::new(keys::NOTHING_TO_UNDO))?;
        self.replay(seq, &json, 1)
    }

    /// Re-applies the oldest undone command.
    ///
    /// # Errors
    /// `errors.history.nothingToRedo`, or storage errors.
    pub fn redo(&mut self) -> Result<ChangeSet, StoreError> {
        let entry: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT seq, command FROM command_log WHERE undone = 1 ORDER BY seq ASC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (seq, json) = entry.ok_or_else(|| CommandError::new(keys::NOTHING_TO_REDO))?;
        self.replay(seq, &json, 0)
    }

    fn replay(&mut self, seq: i64, json: &str, undone: i64) -> Result<ChangeSet, StoreError> {
        let cmd: Command = serde_json::from_str(json)?;
        let rollback = cmd.inverse(&self.project);
        let changes = cmd.apply(&mut self.project)?;
        let result = (|| -> Result<(), StoreError> {
            let tx = self.conn.transaction()?;
            tx.execute(
                "UPDATE command_log SET undone = ?1 WHERE seq = ?2",
                params![undone, seq],
            )?;
            persist_modules(&tx, &self.project, &changes.module_ids)?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = rollback.apply(&mut self.project);
            return Err(e);
        }
        Ok(changes)
    }

    /// Whether `undo` / `redo` would do something.
    ///
    /// # Errors
    /// SQLite errors.
    pub fn history_state(&self) -> Result<(bool, bool), StoreError> {
        let (undo, redo): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(undone = 0), 0), COALESCE(SUM(undone = 1), 0) FROM command_log",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((undo > 0, redo > 0))
    }

    /// The `PVS1` buffer of all modules.
    #[must_use]
    pub fn module_buffer(&self) -> Vec<u8> {
        buffer::encode(&self.project, None)
    }

    /// The `PVS1` buffer of the given modules only.
    #[must_use]
    pub fn module_buffer_for(&self, ids: &[ModuleId]) -> Vec<u8> {
        buffer::encode(&self.project, Some(ids))
    }
}

fn persist_modules(
    tx: &Transaction<'_>,
    project: &Project,
    ids: &[ModuleId],
) -> Result<(), StoreError> {
    let mut stmt =
        tx.prepare_cached("UPDATE module_instance SET u = ?1, v = ?2, state = ?3 WHERE id = ?4")?;
    for id in ids {
        let m = &project.modules[id.index()];
        stmt.execute(params![m.u, m.v, state_str(m.state), m.id.0])?;
    }
    Ok(())
}

fn insert_project(tx: &Transaction<'_>, p: &Project) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO project (id, name) VALUES (?1, ?2)",
        params![p.id.0.to_string(), p.name],
    )?;
    for (ord, b) in p.buildings.iter().enumerate() {
        tx.execute(
            "INSERT INTO building (id, project_id, ord, name, origin_x, origin_y, origin_z, size_x, size_y, eave_height_m, ridge_height_m, roof_kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                b.id.0.to_string(), p.id.0.to_string(), ord as i64, b.name,
                b.origin[0], b.origin[1], b.origin[2], b.size_m[0], b.size_m[1],
                b.eave_height_m, b.ridge_height_m,
                match b.roof_kind { RoofKind::Gable => "gable", RoofKind::Flat => "flat" },
            ],
        )?;
    }
    for (ord, f) in p.roof_faces.iter().enumerate() {
        tx.execute(
            "INSERT INTO roof_face (id, building_id, ord, name, origin_x, origin_y, origin_z, tilt_deg, azimuth_deg, width_m, height_m)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                f.id.0.to_string(), f.building_id.0.to_string(), ord as i64, f.name,
                f.origin[0], f.origin[1], f.origin[2], f.tilt_deg, f.azimuth_deg, f.width_m, f.height_m,
            ],
        )?;
    }
    for (ord, a) in p.arrays.iter().enumerate() {
        tx.execute(
            r#"INSERT INTO "array" (id, roof_face_id, ord, module_length_m, module_width_m, orientation, tilt_on_roof_deg, azimuth_offset_deg,
                                    grid_rows, grid_cols, grid_origin_u, grid_origin_v, grid_pitch_u, grid_pitch_v)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            params![
                a.id.0.to_string(), a.roof_face_id.0.to_string(), ord as i64,
                a.module_length_m, a.module_width_m,
                match a.orientation { Orientation::Portrait => "portrait", Orientation::Landscape => "landscape" },
                a.tilt_on_roof_deg, a.azimuth_offset_deg,
                a.grid.rows, a.grid.cols, a.grid.origin_u, a.grid.origin_v, a.grid.pitch_u, a.grid.pitch_v,
            ],
        )?;
    }
    let mut stmt = tx.prepare(
        "INSERT INTO module_instance (id, array_id, grid_row, grid_col, u, v, state, string_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for m in &p.modules {
        stmt.execute(params![
            m.id.0,
            m.array_id.0.to_string(),
            m.row,
            m.col,
            m.u,
            m.v,
            state_str(m.state),
            m.string_index,
        ])?;
    }
    Ok(())
}

fn load_project(conn: &Connection) -> Result<Project, StoreError> {
    let (id, name): (Uuid, String) = conn
        .query_row("SELECT id, name FROM project LIMIT 1", [], |r| {
            Ok((uuid_col(r, 0)?, r.get(1)?))
        })
        .optional()?
        .ok_or_else(|| StoreError::Corrupt("no project row".into()))?;

    let buildings = conn
        .prepare(
            "SELECT id, name, origin_x, origin_y, origin_z, size_x, size_y, eave_height_m, ridge_height_m, roof_kind
             FROM building ORDER BY ord",
        )?
        .query_map([], |r| {
            Ok(Building {
                id: BuildingId(uuid_col(r, 0)?),
                name: r.get(1)?,
                origin: [r.get(2)?, r.get(3)?, r.get(4)?],
                size_m: [r.get(5)?, r.get(6)?],
                eave_height_m: r.get(7)?,
                ridge_height_m: r.get(8)?,
                roof_kind: enum_col(r, 9, |s| match s {
                    "gable" => Some(RoofKind::Gable),
                    "flat" => Some(RoofKind::Flat),
                    _ => None,
                })?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let roof_faces = conn
        .prepare(
            "SELECT id, building_id, name, origin_x, origin_y, origin_z, tilt_deg, azimuth_deg, width_m, height_m
             FROM roof_face ORDER BY ord",
        )?
        .query_map([], |r| {
            Ok(RoofFace {
                id: RoofFaceId(uuid_col(r, 0)?),
                building_id: BuildingId(uuid_col(r, 1)?),
                name: r.get(2)?,
                origin: [r.get(3)?, r.get(4)?, r.get(5)?],
                tilt_deg: r.get(6)?,
                azimuth_deg: r.get(7)?,
                width_m: r.get(8)?,
                height_m: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let arrays = conn
        .prepare(
            r#"SELECT id, roof_face_id, module_length_m, module_width_m, orientation, tilt_on_roof_deg, azimuth_offset_deg,
                      grid_rows, grid_cols, grid_origin_u, grid_origin_v, grid_pitch_u, grid_pitch_v
               FROM "array" ORDER BY ord"#,
        )?
        .query_map([], |r| {
            Ok(PanelArray {
                id: ArrayId(uuid_col(r, 0)?),
                roof_face_id: RoofFaceId(uuid_col(r, 1)?),
                module_length_m: r.get(2)?,
                module_width_m: r.get(3)?,
                orientation: enum_col(r, 4, |s| match s {
                    "portrait" => Some(Orientation::Portrait),
                    "landscape" => Some(Orientation::Landscape),
                    _ => None,
                })?,
                tilt_on_roof_deg: r.get(5)?,
                azimuth_offset_deg: r.get(6)?,
                grid: ArrayGrid {
                    rows: r.get(7)?,
                    cols: r.get(8)?,
                    origin_u: r.get(9)?,
                    origin_v: r.get(10)?,
                    pitch_u: r.get(11)?,
                    pitch_v: r.get(12)?,
                },
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let modules = conn
        .prepare(
            "SELECT id, array_id, grid_row, grid_col, u, v, state, string_index FROM module_instance ORDER BY id",
        )?
        .query_map([], |r| {
            Ok(ModuleInstance {
                id: ModuleId(r.get(0)?),
                array_id: ArrayId(uuid_col(r, 1)?),
                row: r.get(2)?,
                col: r.get(3)?,
                u: r.get(4)?,
                v: r.get(5)?,
                state: enum_col(r, 6, |s| match s {
                    "active" => Some(ModuleState::Active),
                    "removed" => Some(ModuleState::Removed),
                    _ => None,
                })?,
                string_index: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Project {
        id: ProjectId(id),
        name,
        buildings,
        roof_faces,
        arrays,
        modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_project;

    fn ids(v: &[u32]) -> Vec<ModuleId> {
        v.iter().map(|&i| ModuleId(i)).collect()
    }

    fn new_store(dir: &tempfile::TempDir) -> ProjectStore {
        ProjectStore::create(&dir.path().join("p.pvproj"), fixture_project()).expect("create")
    }

    #[test]
    fn create_then_open_round_trips_the_model() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = new_store(&dir);
        let original = store.project().clone();
        drop(store);
        let reopened = ProjectStore::open(&dir.path().join("p.pvproj")).expect("open");
        assert_eq!(reopened.project(), &original);
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let dir = tempfile::tempdir().expect("tmp");
        drop(new_store(&dir));
        let again = ProjectStore::create(&dir.path().join("p.pvproj"), fixture_project());
        assert!(matches!(again, Err(StoreError::AlreadyExists(_))));
    }

    #[test]
    fn undo_redo_sequence_with_branch_truncation() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut s = new_store(&dir);
        let s0 = s.project().clone();
        assert_eq!(s.history_state().expect("h"), (false, false));

        let mv = Command::MoveModules {
            ids: ids(&[0, 1]),
            du: 0.1,
            dv: 0.05,
        };
        s.apply(&mv, Author::User).expect("move");
        let s1 = s.project().clone();
        s.apply(&Command::RemoveModules { ids: ids(&[3]) }, Author::User)
            .expect("remove");
        let s2 = s.project().clone();
        assert_eq!(s.history_state().expect("h"), (true, false));

        s.undo().expect("undo remove");
        assert_eq!(s.project(), &s1);
        s.undo().expect("undo move");
        assert_eq!(s.project(), &s0, "bit-exact after two undos");
        assert_eq!(s.history_state().expect("h"), (false, true));
        let err = s.undo().expect_err("nothing left");
        assert!(matches!(err, StoreError::Command(ref e) if e.key == keys::NOTHING_TO_UNDO));

        s.redo().expect("redo move");
        assert_eq!(s.project(), &s1);
        s.redo().expect("redo remove");
        assert_eq!(s.project(), &s2);
        assert!(
            matches!(s.redo(), Err(StoreError::Command(ref e)) if e.key == keys::NOTHING_TO_REDO)
        );

        // Undo once, then a new command: the redo branch must be dropped.
        s.undo().expect("undo remove");
        s.apply(&Command::RemoveModules { ids: ids(&[4]) }, Author::User)
            .expect("new branch");
        assert_eq!(s.history_state().expect("h"), (true, false));
        let undone: i64 = s
            .conn
            .query_row(
                "SELECT COUNT(*) FROM command_log WHERE undone = 1",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(undone, 0);

        // Journal survives re-opening.
        let after = s.project().clone();
        drop(s);
        let mut s = ProjectStore::open(&dir.path().join("p.pvproj")).expect("reopen");
        assert_eq!(s.project(), &after);
        s.undo().expect("undo after reopen");
        s.undo().expect("undo move after reopen");
        assert_eq!(s.project(), &s0);
    }

    #[test]
    fn rejected_command_is_not_journalled() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut s = new_store(&dir);
        let before = s.project().clone();
        let err = s
            .apply(
                &Command::MoveModules {
                    ids: ids(&[0]),
                    du: -50.0,
                    dv: 0.0,
                },
                Author::User,
            )
            .expect_err("out of roof");
        assert!(matches!(err, StoreError::Command(ref e) if e.key == keys::OUT_OF_ROOF));
        assert_eq!(s.project(), &before);
        assert_eq!(s.history_state().expect("h"), (false, false));
    }

    #[test]
    fn journal_columns() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut s = new_store(&dir);
        s.apply(&Command::RemoveModules { ids: ids(&[1]) }, Author::Copilot)
            .expect("remove");
        let (author, command, inverse, undone): (String, String, String, i64) = s
            .conn
            .query_row(
                "SELECT author, command, inverse, undone FROM command_log",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("row");
        assert_eq!(author, "copilot");
        assert_eq!(command, r#"{"type":"RemoveModules","ids":[1]}"#);
        assert_eq!(inverse, r#"{"type":"RestoreModules","ids":[1]}"#);
        assert_eq!(undone, 0);
    }
}
