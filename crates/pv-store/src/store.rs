//! Open project + command journal (undo/redo) backed by one SQLite file.

use std::path::{Path, PathBuf};

use pv_model::command::keys;
use pv_model::component::legacy::adopt_legacy_mounting;
use pv_model::{
    ArrayGrid, ArrayId, ArrayLayoutParams, Building, BuildingId, ChangeSet, Command, CommandError,
    ComponentId, ComponentSnapshot, ModuleId, ModuleInstance, ModuleState, MountingSystem,
    Orientation, PanelArray, Pricing, Project, ProjectId, RoofFace, RoofFaceId, RoofKind,
};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use uuid::Uuid;

use crate::{buffer, schema, StoreError};

/// A frozen bill of materials (its JSON is read with `bom_snapshot_json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomSnapshotRecord {
    pub id: i64,
    pub created_at: String,
    pub label: String,
    pub seq: u32,
    pub input_fingerprint: String,
}

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

/// Optional JSON column (NULL → `None`).
fn json_col<T: serde::de::DeserializeOwned>(
    row: &Row<'_>,
    idx: usize,
) -> rusqlite::Result<Option<T>> {
    let s: Option<String> = row.get(idx)?;
    s.map(|s| {
        serde_json::from_str(&s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
        })
    })
    .transpose()
}

/// JSON object column merged onto `T::default()`: keys missing in the stored JSON (rows
/// written before a field existed, e.g. the v2 migration default `'{}'`) take their
/// default value. Keeps the model types free of `#[serde(default)]`.
fn json_with_defaults<T>(row: &Row<'_>, idx: usize) -> rusqlite::Result<T>
where
    T: Default + serde::Serialize + serde::de::DeserializeOwned,
{
    let conv = |e: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    };
    let stored: serde_json::Value =
        serde_json::from_str(&row.get::<_, String>(idx)?).map_err(conv)?;
    let mut merged = serde_json::to_value(T::default()).map_err(conv)?;
    if let (Some(base), serde_json::Value::Object(over)) = (merged.as_object_mut(), stored) {
        base.extend(over);
    }
    serde_json::from_value(merged).map_err(conv)
}

fn roof_kind_str(k: RoofKind) -> &'static str {
    match k {
        RoofKind::Gable => "gable",
        RoofKind::Flat => "flat",
        RoofKind::Hipped => "hipped",
        RoofKind::Monopitch => "monopitch",
    }
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
        let (mut project, legacy) = load_project(&conn)?;
        convert_legacy(&mut conn, &mut project, &legacy)?;
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
        if changes.is_empty() {
            return Ok(changes);
        }
        // Layout rules on the resulting state (setbacks, obstacles, collisions, distance
        // between arrays). Commands that only remove modules cannot break them.
        if let Err(e) = validate_layout(&self.project, cmd, &changes) {
            let _ = inverse.apply(&mut self.project);
            return Err(StoreError::Command(e));
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
            persist(&tx, &self.project, &changes)?;
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
            persist(&tx, &self.project, &changes)?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = rollback.apply(&mut self.project);
            return Err(e);
        }
        Ok(changes)
    }

    /// Records a frozen bill of materials (not a model change: not journalled, never
    /// modified). `bom_json` is the serialised `pv_bom::Bom`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn save_bom_snapshot(
        &self,
        label: &str,
        seq: u32,
        input_fingerprint: &str,
        bom_json: &str,
    ) -> Result<BomSnapshotRecord, StoreError> {
        self.conn.execute(
            "INSERT INTO bom_snapshot (label, seq, input_fingerprint, pricing_json, bom_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                label,
                seq,
                input_fingerprint,
                serde_json::to_string(&self.project.pricing)?,
                bom_json
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.bom_snapshots()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| StoreError::Corrupt("bom snapshot not found".into()))
    }

    /// Frozen bills of materials, oldest first.
    ///
    /// # Errors
    /// Storage errors.
    pub fn bom_snapshots(&self) -> Result<Vec<BomSnapshotRecord>, StoreError> {
        Ok(self
            .conn
            .prepare("SELECT id, created_at, label, seq, input_fingerprint FROM bom_snapshot ORDER BY id")?
            .query_map([], |r| {
                Ok(BomSnapshotRecord {
                    id: r.get(0)?,
                    created_at: r.get(1)?,
                    label: r.get(2)?,
                    seq: r.get(3)?,
                    input_fingerprint: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// The JSON of a frozen bill of materials and the pricing it was computed with.
    ///
    /// # Errors
    /// Storage errors; `None` for an unknown id.
    pub fn bom_snapshot_json(&self, id: i64) -> Result<Option<(String, String)>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT bom_json, pricing_json FROM bom_snapshot WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
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

/// Layout rules on the state after `cmd`:
/// - moved modules: setbacks, obstacles, collisions, distance between arrays;
/// - changed faces and arrays: the outline must give a buildable area for each array;
/// - new modules (structural change): the same rules as moved ones, for the active modules
///   of the changed arrays.
///
/// Editing a face or an array does not re-check the modules already on it: the statics and
/// layout reports show what no longer fits, the user decides.
fn validate_layout(
    project: &Project,
    cmd: &Command,
    changes: &ChangeSet,
) -> Result<(), CommandError> {
    if !changes.module_ids.is_empty() && !matches!(cmd, Command::RemoveModules { .. }) {
        pv_layout::validate_module_positions(project, &changes.module_ids)?;
    }
    if changes.modules_only() {
        return Ok(());
    }
    let arrays: Vec<&PanelArray> = project
        .arrays
        .iter()
        .filter(|a| changes.arrays.contains(&a.id) || changes.faces.contains(&a.roof_face_id))
        .collect();
    for a in &arrays {
        if let Some(face) = project.roof_face(a.roof_face_id) {
            pv_layout::model::buildable_area(face, a)
                .map_err(|_| CommandError::new(keys::LAYOUT_INVALID_OUTLINE))?;
        }
    }
    let mut laid_out = Vec::new();
    laid_out_arrays(cmd, &mut laid_out);
    if !laid_out.is_empty() {
        let new: Vec<ModuleId> = project
            .modules
            .iter()
            .filter(|m| m.state == ModuleState::Active && laid_out.contains(&m.array_id))
            .map(|m| m.id)
            .collect();
        pv_layout::validate_module_positions(project, &new)?;
    }
    Ok(())
}

/// Arrays whose modules `cmd` replaces (`SetArrayModules`, also inside batches).
fn laid_out_arrays(cmd: &Command, out: &mut Vec<ArrayId>) {
    match cmd {
        Command::SetArrayModules { array_id, .. } => out.push(*array_id),
        Command::Batch { commands } => commands.iter().for_each(|c| laid_out_arrays(c, out)),
        _ => {}
    }
}

/// Writes what `changes` touched.
fn persist(tx: &Transaction<'_>, project: &Project, changes: &ChangeSet) -> Result<(), StoreError> {
    if changes.pricing {
        tx.execute(
            "UPDATE project SET pricing_json = ?1",
            params![serde_json::to_string(&project.pricing)?],
        )?;
    }
    if changes.components {
        write_components(tx, project)?;
    }
    if !changes.modules_only() {
        persist_entities(tx, project, changes)?;
    }
    if changes.structure {
        // Ids were renumbered: the module table is rewritten.
        tx.execute("DELETE FROM module_instance", [])?;
        insert_modules(tx, &project.modules)?;
        return Ok(());
    }
    let mut stmt =
        tx.prepare_cached("UPDATE module_instance SET u = ?1, v = ?2, state = ?3 WHERE id = ?4")?;
    for id in &changes.module_ids {
        let m = &project.modules[id.index()];
        stmt.execute(params![m.u, m.v, state_str(m.state), m.id.0])?;
    }
    Ok(())
}

/// Entities added, changed or removed, then the `ord` columns of all entities rewritten.
/// Rows are upserted (`ON CONFLICT … DO UPDATE`), never replaced: `REPLACE` deletes the row
/// first, which would cascade to its faces, arrays and modules.
fn persist_entities(
    tx: &Transaction<'_>,
    project: &Project,
    changes: &ChangeSet,
) -> Result<(), StoreError> {
    // Removals (children first; the foreign keys cascade anyway).
    for id in &changes.arrays {
        if project.array_index(*id).is_none() {
            tx.execute(
                r#"DELETE FROM "array" WHERE id = ?1"#,
                params![id.0.to_string()],
            )?;
        }
    }
    for id in &changes.faces {
        if project.roof_face(*id).is_none() {
            tx.execute(
                "DELETE FROM roof_face WHERE id = ?1",
                params![id.0.to_string()],
            )?;
        }
    }
    for id in &changes.buildings {
        if !project.buildings.iter().any(|b| b.id == *id) {
            tx.execute(
                "DELETE FROM building WHERE id = ?1",
                params![id.0.to_string()],
            )?;
        }
    }
    // Free every ord (`"array".ord` is UNIQUE) before writing the final ones.
    for table in ["building", "roof_face", r#""array""#] {
        tx.execute(&format!("UPDATE {table} SET ord = -ord - 1"), [])?;
    }
    for (ord, b) in project.buildings.iter().enumerate() {
        if changes.buildings.contains(&b.id) {
            upsert_building(tx, project.id, ord, b)?;
        }
    }
    for (ord, f) in project.roof_faces.iter().enumerate() {
        if changes.faces.contains(&f.id) {
            upsert_face(tx, ord, f)?;
        }
    }
    for (ord, a) in project.arrays.iter().enumerate() {
        if changes.arrays.contains(&a.id) {
            upsert_array(tx, ord, a)?;
        }
    }
    for (table, ids) in [
        (
            "building",
            project.buildings.iter().map(|b| b.id.0).collect::<Vec<_>>(),
        ),
        (
            "roof_face",
            project.roof_faces.iter().map(|f| f.id.0).collect(),
        ),
        (
            r#""array""#,
            project.arrays.iter().map(|a| a.id.0).collect(),
        ),
    ] {
        let mut stmt = tx.prepare_cached(&format!("UPDATE {table} SET ord = ?1 WHERE id = ?2"))?;
        for (ord, id) in ids.iter().enumerate() {
            stmt.execute(params![ord as i64, id.to_string()])?;
        }
    }
    Ok(())
}

fn upsert_building(
    tx: &Transaction<'_>,
    project: ProjectId,
    ord: usize,
    b: &Building,
) -> Result<(), StoreError> {
    tx.prepare_cached(
        "INSERT INTO building (id, project_id, ord, name, origin_x, origin_y, origin_z, size_x, size_y, eave_height_m, ridge_height_m, roof_kind, site_json, footprint_json, roof_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id, ord = excluded.ord, name = excluded.name,
             origin_x = excluded.origin_x, origin_y = excluded.origin_y, origin_z = excluded.origin_z,
             size_x = excluded.size_x, size_y = excluded.size_y, eave_height_m = excluded.eave_height_m,
             ridge_height_m = excluded.ridge_height_m, roof_kind = excluded.roof_kind, site_json = excluded.site_json,
             footprint_json = excluded.footprint_json, roof_json = excluded.roof_json",
    )?
    .execute(params![
        b.id.0.to_string(), project.0.to_string(), ord as i64, b.name,
        b.origin[0], b.origin[1], b.origin[2], b.size_m[0], b.size_m[1],
        b.eave_height_m, b.ridge_height_m,
        roof_kind_str(b.roof_kind),
        b.site.as_ref().map(serde_json::to_string).transpose()?,
        serde_json::to_string(&b.footprint)?,
        b.roof.as_ref().map(serde_json::to_string).transpose()?,
    ])?;
    Ok(())
}

fn upsert_face(tx: &Transaction<'_>, ord: usize, f: &RoofFace) -> Result<(), StoreError> {
    tx.prepare_cached(
        "INSERT INTO roof_face (id, building_id, ord, name, origin_x, origin_y, origin_z, tilt_deg, azimuth_deg, width_m, height_m, outline_json, support_grid_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET building_id = excluded.building_id, ord = excluded.ord, name = excluded.name,
             origin_x = excluded.origin_x, origin_y = excluded.origin_y, origin_z = excluded.origin_z,
             tilt_deg = excluded.tilt_deg, azimuth_deg = excluded.azimuth_deg, width_m = excluded.width_m,
             height_m = excluded.height_m, outline_json = excluded.outline_json, support_grid_json = excluded.support_grid_json",
    )?
    .execute(params![
        f.id.0.to_string(), f.building_id.0.to_string(), ord as i64, f.name,
        f.origin[0], f.origin[1], f.origin[2], f.tilt_deg, f.azimuth_deg, f.width_m, f.height_m,
        f.outline.as_ref().map(serde_json::to_string).transpose()?,
        f.support_grid.as_ref().map(serde_json::to_string).transpose()?,
    ])?;
    Ok(())
}

fn upsert_array(tx: &Transaction<'_>, ord: usize, a: &PanelArray) -> Result<(), StoreError> {
    tx.prepare_cached(
        r#"INSERT INTO "array" (id, roof_face_id, ord, module_length_m, module_width_m, orientation, tilt_on_roof_deg, azimuth_offset_deg,
                                grid_rows, grid_cols, grid_origin_u, grid_origin_v, grid_pitch_u, grid_pitch_v,
                                grid_block_cols, grid_block_rows, grid_block_extra_u, grid_block_extra_v, layout_json, mounting_json,
                                module_ref, mounting_ref)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, NULL, ?20, ?21)
           ON CONFLICT(id) DO UPDATE SET roof_face_id = excluded.roof_face_id, ord = excluded.ord,
               module_length_m = excluded.module_length_m, module_width_m = excluded.module_width_m,
               orientation = excluded.orientation, tilt_on_roof_deg = excluded.tilt_on_roof_deg,
               azimuth_offset_deg = excluded.azimuth_offset_deg, grid_rows = excluded.grid_rows,
               grid_cols = excluded.grid_cols, grid_origin_u = excluded.grid_origin_u,
               grid_origin_v = excluded.grid_origin_v, grid_pitch_u = excluded.grid_pitch_u,
               grid_pitch_v = excluded.grid_pitch_v, grid_block_cols = excluded.grid_block_cols,
               grid_block_rows = excluded.grid_block_rows, grid_block_extra_u = excluded.grid_block_extra_u,
               grid_block_extra_v = excluded.grid_block_extra_v, layout_json = excluded.layout_json,
               mounting_json = NULL, module_ref = excluded.module_ref, mounting_ref = excluded.mounting_ref"#,
    )?
    .execute(params![
        a.id.0.to_string(), a.roof_face_id.0.to_string(), ord as i64,
        a.module_length_m, a.module_width_m,
        match a.orientation { Orientation::Portrait => "portrait", Orientation::Landscape => "landscape" },
        a.tilt_on_roof_deg, a.azimuth_offset_deg,
        a.grid.rows, a.grid.cols, a.grid.origin_u, a.grid.origin_v, a.grid.pitch_u, a.grid.pitch_v,
        a.grid.block_cols, a.grid.block_rows, a.grid.block_extra_u, a.grid.block_extra_v,
        serde_json::to_string(&a.layout)?,
        a.module.map(|id| id.to_string()),
        a.mounting.map(|id| id.to_string()),
    ])?;
    Ok(())
}

fn insert_modules(tx: &Transaction<'_>, modules: &[ModuleInstance]) -> Result<(), StoreError> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO module_instance (id, array_id, grid_row, grid_col, u, v, state, string_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for m in modules {
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

fn insert_project(tx: &Transaction<'_>, p: &Project) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO project (id, name, pricing_json) VALUES (?1, ?2, ?3)",
        params![
            p.id.0.to_string(),
            p.name,
            serde_json::to_string(&p.pricing)?
        ],
    )?;
    for (ord, b) in p.buildings.iter().enumerate() {
        upsert_building(tx, p.id, ord, b)?;
    }
    for (ord, f) in p.roof_faces.iter().enumerate() {
        upsert_face(tx, ord, f)?;
    }
    for (ord, a) in p.arrays.iter().enumerate() {
        upsert_array(tx, ord, a)?;
    }
    write_components(tx, p)?;
    insert_modules(tx, &p.modules)
}

/// The project, and the old mounting snapshot of each array still to convert (files written
/// before schema v8).
fn load_project(conn: &Connection) -> Result<(Project, Vec<Option<MountingSystem>>), StoreError> {
    let (id, name, pricing): (Uuid, String, Pricing) = conn
        .query_row(
            "SELECT id, name, pricing_json FROM project LIMIT 1",
            [],
            |r| {
                Ok((
                    uuid_col(r, 0)?,
                    r.get(1)?,
                    json_with_defaults::<Pricing>(r, 2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::Corrupt("no project row".into()))?;

    let buildings = conn
        .prepare(
            "SELECT id, name, origin_x, origin_y, origin_z, size_x, size_y, eave_height_m, ridge_height_m, roof_kind, site_json, footprint_json, roof_json
             FROM building ORDER BY ord",
        )?
        .query_map([], |r| {
            let size_m: [f64; 2] = [r.get(5)?, r.get(6)?];
            let mut footprint: Vec<[f64; 2]> = json_col(r, 11)?.unwrap_or_default();
            if footprint.is_empty() {
                // Written before v7: the rectangle the size described.
                footprint = pv_model::rectangle_footprint(size_m);
            }
            Ok(Building {
                id: BuildingId(uuid_col(r, 0)?),
                name: r.get(1)?,
                origin: [r.get(2)?, r.get(3)?, r.get(4)?],
                footprint,
                size_m,
                eave_height_m: r.get(7)?,
                ridge_height_m: r.get(8)?,
                roof_kind: enum_col(r, 9, |s| match s {
                    "gable" => Some(RoofKind::Gable),
                    "flat" => Some(RoofKind::Flat),
                    "hipped" => Some(RoofKind::Hipped),
                    "monopitch" => Some(RoofKind::Monopitch),
                    _ => None,
                })?,
                roof: json_col(r, 12)?,
                site: json_col(r, 10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let roof_faces = conn
        .prepare(
            "SELECT id, building_id, name, origin_x, origin_y, origin_z, tilt_deg, azimuth_deg, width_m, height_m, outline_json, support_grid_json
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
                outline: json_col(r, 10)?,
                support_grid: json_col(r, 11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let arrays = conn
        .prepare(
            r#"SELECT id, roof_face_id, module_length_m, module_width_m, orientation, tilt_on_roof_deg, azimuth_offset_deg,
                      grid_rows, grid_cols, grid_origin_u, grid_origin_v, grid_pitch_u, grid_pitch_v,
                      grid_block_cols, grid_block_rows, grid_block_extra_u, grid_block_extra_v, layout_json, mounting_json,
                      module_ref, mounting_ref
               FROM "array" ORDER BY ord"#,
        )?
        .query_map([], |r| {
            let legacy: Option<MountingSystem> = json_col(r, 18)?;
            let module = r
                .get::<_, Option<String>>(19)?
                .map(|_| uuid_col(r, 19).map(ComponentId))
                .transpose()?;
            let mounting = r
                .get::<_, Option<String>>(20)?
                .map(|_| uuid_col(r, 20).map(ComponentId))
                .transpose()?;
            // Old snapshot only when the array has no component yet.
            let legacy = legacy.filter(|_| mounting.is_none());
            Ok((legacy, PanelArray {
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
                    block_cols: r.get(13)?,
                    block_rows: r.get(14)?,
                    block_extra_u: r.get(15)?,
                    block_extra_v: r.get(16)?,
                },
                layout: json_with_defaults::<ArrayLayoutParams>(r, 17)?,
                module,
                mounting,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let (legacy, arrays): (Vec<Option<MountingSystem>>, Vec<PanelArray>) =
        arrays.into_iter().unzip();

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

    Ok((
        Project {
            id: ProjectId(id),
            name,
            buildings,
            roof_faces,
            arrays,
            modules,
            pricing,
            components: load_components(conn)?,
        },
        legacy,
    ))
}

/// Rewrites the frozen components.
fn write_components(tx: &Transaction<'_>, project: &Project) -> Result<(), StoreError> {
    tx.execute("DELETE FROM component_snapshot", [])?;
    let mut stmt = tx.prepare_cached(
        "INSERT INTO component_snapshot (id, revision, component_json, price_json) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for s in &project.components {
        stmt.execute(params![
            s.component.id.to_string(),
            s.component.revision,
            serde_json::to_string(&s.component)?,
            s.price.as_ref().map(serde_json::to_string).transpose()?,
        ])?;
    }
    Ok(())
}

fn load_components(conn: &Connection) -> Result<Vec<ComponentSnapshot>, StoreError> {
    let rows = conn
        .prepare("SELECT component_json, price_json FROM component_snapshot ORDER BY id")?
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = rows
        .into_iter()
        .map(|(c, p)| {
            Ok(ComponentSnapshot {
                component: serde_json::from_str(&c)?,
                price: p.as_deref().map(serde_json::from_str).transpose()?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    // Canonical order of `Project::components` (the text order of ids is the UUID order).
    out.sort_by_key(|s| s.component.id);
    Ok(out)
}

/// Converts the old mounting snapshots (schema ≤ v7) into frozen components, and writes the
/// result. Returns whether anything was converted.
fn convert_legacy(
    conn: &mut Connection,
    project: &mut Project,
    legacy: &[Option<MountingSystem>],
) -> Result<bool, StoreError> {
    let mut converted = false;
    for (i, m) in legacy.iter().enumerate() {
        if let Some(m) = m {
            adopt_legacy_mounting(project, i, m)?;
            converted = true;
        }
    }
    if converted {
        let tx = conn.transaction()?;
        write_components(&tx, project)?;
        for (ord, a) in project.arrays.iter().enumerate() {
            upsert_array(&tx, ord, a)?;
        }
        // Journaled commands carry the old snapshot format: history starts afresh.
        tx.execute("DELETE FROM command_log", [])?;
        tx.commit()?;
    }
    Ok(converted)
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

    /// v4: site per building, mounting snapshot per array, the new roof kinds.
    #[test]
    fn statics_data_round_trips() {
        use pv_model::{
            CpEntrySpec, CpSpec, FixationSpec, MountingSystem, SiteParams, TerrainCategory,
            WindZone,
        };
        let dir = tempfile::tempdir().expect("tmp");
        let mut p = fixture_project();
        p.buildings[0].roof_kind = RoofKind::Hipped;
        p.buildings[1].roof_kind = RoofKind::Monopitch;
        let mut site = SiteParams::new(389.88, 1.1);
        site.terrain = TerrainCategory::IV;
        p.buildings[0].site = Some(site);
        let mut m = MountingSystem::k2_single_rail();
        m.psi0_snow = Some(0.86);
        m.cp = CpSpec::Explicit {
            reference: "K2 Base".into(),
            entries: vec![CpEntrySpec {
                zone: WindZone::Ridge,
                cp_pressure: 0.2,
                cp_suction: -1.0,
            }],
        };
        if let Some(r) = m.rails.as_mut() {
            r.fixation = Some(FixationSpec {
                name: "HangerBolt".into(),
                uplift_kn: 3.48,
                pressure_kn: 2.71,
                parallel_kn: 0.34,
            });
        }
        adopt_legacy_mounting(&mut p, 0, &m).expect("components");
        let path = dir.path().join("statics.pvproj");
        drop(ProjectStore::create(&path, p.clone()).expect("create"));
        let reopened = ProjectStore::open(&path).expect("open");
        assert_eq!(reopened.project(), &p);
        let p = reopened.project();
        assert_eq!(p.resolve_mounting(&p.arrays[0]).expect("resolve"), Some(m));
    }

    /// Writes `m` as a pre-library snapshot (schema v7) on array 0, with a journal entry.
    fn v7_file_with_snapshot(path: &Path, m: &MountingSystem) -> Project {
        let original = fixture_project();
        let mut s = ProjectStore::create(path, original.clone()).expect("create");
        s.apply(
            &Command::SetDiscount {
                manufacturer: "K2 Systems".into(),
                rate: Some(0.3),
            },
            Author::User,
        )
        .expect("journal entry");
        let expected = s.project().clone();
        drop(s);
        let conn = Connection::open(path).expect("raw open");
        conn.execute(
            r#"UPDATE "array" SET mounting_json = ?1 WHERE ord = 0"#,
            [serde_json::to_string(m).expect("json")],
        )
        .expect("snapshot");
        conn.execute_batch(
            r#"DROP TABLE component_snapshot;
               ALTER TABLE "array" DROP COLUMN module_ref;
               ALTER TABLE "array" DROP COLUMN mounting_ref;
               PRAGMA user_version = 7;"#,
        )
        .expect("downgrade to v7");
        expected
    }

    /// v8: old snapshots become frozen components resolving to the same system; the undo
    /// journal (old command format) is cleared; reopening converts nothing more.
    #[test]
    fn v7_snapshot_becomes_components() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("v7.pvproj");
        let m = MountingSystem::k2_single_rail();
        let before = v7_file_with_snapshot(&path, &m);
        let s = ProjectStore::open(&path).expect("open v7");
        let p = s.project().clone();
        assert_eq!(p.resolve_mounting(&p.arrays[0]).expect("resolve"), Some(m));
        assert_eq!(p.arrays[1].mounting, None);
        assert_eq!(p.components.len(), 7, "5 parts, module, system");
        assert_eq!(p.pricing, before.pricing);
        assert_eq!(s.history_state().expect("history"), (false, false));
        drop(s);
        let again = ProjectStore::open(&path).expect("reopen");
        assert_eq!(again.project(), &p, "stable: ids derived from content");
    }

    /// A v3 file (before the statics columns) migrates: the building table is rebuilt
    /// without cascading to the roof faces, arrays and modules, and foreign keys are back on.
    #[test]
    fn v3_file_upgrades_without_losing_children() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("v3.pvproj");
        let original = fixture_project();
        drop(ProjectStore::create(&path, original.clone()).expect("create"));
        {
            let conn = Connection::open(&path).expect("raw open");
            conn.execute_batch(
                r#"ALTER TABLE building DROP COLUMN site_json;
                   ALTER TABLE "array" DROP COLUMN mounting_json;
                   ALTER TABLE project DROP COLUMN pricing_json;
                   DROP TABLE bom_snapshot;
                   DROP TABLE component_snapshot;
                   ALTER TABLE "array" DROP COLUMN module_ref;
                   ALTER TABLE "array" DROP COLUMN mounting_ref;
                   PRAGMA user_version = 3;"#,
            )
            .expect("downgrade to v3");
        }
        let reopened = ProjectStore::open(&path).expect("open v3");
        assert_eq!(reopened.project(), &original);
        assert!(!original.roof_faces.is_empty() && !original.modules.is_empty());
        let fk: i64 = reopened
            .conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .expect("fk");
        assert_eq!(fk, 1);
    }

    /// v6: the discount is a journalled command, persisted, undone and redone like any other.
    #[test]
    fn discounts_are_commands() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("pricing.pvproj");
        let mut s = ProjectStore::create(&path, fixture_project()).expect("create");
        let set = |rate| Command::SetDiscount {
            manufacturer: "K2 Systems".into(),
            rate,
        };
        let c = s.apply(&set(Some(0.35)), Author::User).expect("discount");
        assert!(c.pricing);
        assert_eq!(s.history_state().expect("h"), (true, false));
        // The same rate again is not journalled.
        assert!(s
            .apply(&set(Some(0.35)), Author::User)
            .expect("same")
            .is_empty());
        let with = s.project().clone();
        drop(s);
        let mut s = ProjectStore::open(&path).expect("reopen");
        assert_eq!(s.project(), &with);
        assert!(s.undo().expect("undo").pricing);
        assert_eq!(s.project().pricing, pv_model::Pricing::default());
        drop(s);
        let mut s = ProjectStore::open(&path).expect("reopen after undo");
        assert_eq!(s.project().pricing, pv_model::Pricing::default());
        s.redo().expect("redo");
        assert_eq!(s.project(), &with);
    }

    #[test]
    fn bom_snapshots_are_immutable_records() {
        let dir = tempfile::tempdir().expect("tmp");
        let s = new_store(&dir);
        let a = s
            .save_bom_snapshot("Offre 1", 7, "00ff", r#"{"total":1}"#)
            .expect("save");
        let b = s
            .save_bom_snapshot("Offre 2", 9, "0aaa", r#"{"total":2}"#)
            .expect("save");
        assert_eq!((a.label.as_str(), a.seq), ("Offre 1", 7));
        assert!(b.id > a.id);
        assert_eq!(s.bom_snapshots().expect("list"), vec![a.clone(), b]);
        let (bom, pricing) = s.bom_snapshot_json(a.id).expect("read").expect("exists");
        assert_eq!(bom, r#"{"total":1}"#);
        assert_eq!(pricing, r#"{"discounts":[]}"#);
        assert!(s.bom_snapshot_json(999).expect("read").is_none());
    }

    /// v5: a mounting snapshot written before parts existed opens with no parts.
    #[test]
    fn v4_mounting_snapshot_upgrades_without_parts() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("v4.pvproj");
        let original = fixture_project();
        drop(ProjectStore::create(&path, original.clone()).expect("create"));
        {
            let conn = Connection::open(&path).expect("raw open");
            let json = serde_json::to_string(&MountingSystem::k2_single_rail()).expect("json");
            conn.execute(
                r#"UPDATE "array" SET mounting_json = ?1 WHERE ord = 0"#,
                [json],
            )
            .expect("snapshot");
            conn.execute_batch(
                r#"UPDATE "array" SET mounting_json = json_remove(mounting_json, '$.parts')
                     WHERE mounting_json IS NOT NULL;
                   ALTER TABLE project DROP COLUMN pricing_json;
                   DROP TABLE bom_snapshot;
                   ALTER TABLE building DROP COLUMN footprint_json;
                   ALTER TABLE building DROP COLUMN roof_json;
                   DROP TABLE component_snapshot;
                   ALTER TABLE "array" DROP COLUMN module_ref;
                   ALTER TABLE "array" DROP COLUMN mounting_ref;
                   PRAGMA user_version = 4;"#,
            )
            .expect("downgrade to v4");
        }
        let reopened = ProjectStore::open(&path).expect("open v4");
        let p = reopened.project();
        let expected = MountingSystem {
            parts: Vec::new(),
            ..MountingSystem::k2_single_rail()
        };
        assert_eq!(
            p.resolve_mounting(&p.arrays[0]).expect("resolve"),
            Some(expected)
        );
        assert_eq!(p.modules, original.modules);
    }

    #[test]
    fn layout_data_round_trips() {
        use pv_model::{
            BlockParams, EdgeSetback, LayoutAnchor, ObstacleKind, RoofEdgeKind, RoofObstacle,
            RoofOutlineSpec,
        };
        let dir = tempfile::tempdir().expect("tmp");
        let mut p = fixture_project();
        p.roof_faces[0].outline = Some(RoofOutlineSpec {
            vertices_uv: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 4.6], [3.0, 4.6]],
            edge_kinds: vec![
                RoofEdgeKind::Eave,
                RoofEdgeKind::Verge,
                RoofEdgeKind::Ridge,
                RoofEdgeKind::Hip,
            ],
            obstacles: vec![RoofObstacle {
                name: "chimney".into(),
                kind: ObstacleKind::Chimney,
                footprint_uv: vec![[5.0, 2.0], [5.5, 2.0], [5.5, 2.5], [5.0, 2.5]],
                height_m: 1.0,
                clearance_m: Some(0.4),
            }],
        });
        p.roof_faces[0].support_grid = Some(pv_model::SupportGridSpec {
            kind: pv_model::SupportGridKind::StandingSeam,
            origin_m: 0.12,
            spacing_m: 0.6,
        });
        let a = &mut p.arrays[0];
        a.layout.boundary_uv = vec![[0.5, 0.5], [9.0, 0.5], [9.0, 4.0], [0.5, 4.0]];
        a.layout.module_gap_u_m = 0.01;
        a.layout.block = Some(BlockParams {
            max_cols: 2,
            max_rows: 2,
            gap_u_m: 0.08,
            gap_v_m: 0.13,
        });
        a.layout.edge_setbacks = vec![EdgeSetback {
            kind: RoofEdgeKind::Ridge,
            distance_m: 0.4,
        }];
        a.layout.anchor = LayoutAnchor::LowerRight;
        a.layout.min_distance_to_other_arrays_m = 0.3;
        a.layout.rails = Some(pv_model::CrossRailSpec {
            per_column: 2,
            top_inset_m: 0.4,
            top_overhang_m: 0.05,
            bottom_spacing_m: 1.0,
            min_support_overhang_m: 0.06,
            spacing_step_m: 0.01,
            bottom_inset_m: 0.275,
            fixation_edge_m: 0.03,
            max_cantilever_m: Some(0.485),
        });
        a.grid.block_cols = 2;
        a.grid.block_rows = 2;
        a.grid.block_extra_u = 0.07;
        a.grid.block_extra_v = 0.11;
        let path = dir.path().join("layout.pvproj");
        drop(ProjectStore::create(&path, p.clone()).expect("create"));
        assert_eq!(ProjectStore::open(&path).expect("open").project(), &p);
    }

    /// A file written before the layout columns existed (schema v1) opens, migrates and
    /// loads with the defaults.
    #[test]
    fn v1_file_upgrades_with_layout_defaults() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("old.pvproj");
        let original = fixture_project();
        drop(ProjectStore::create(&path, original.clone()).expect("create"));
        {
            let conn = Connection::open(&path).expect("raw open");
            conn.execute_batch(
                r#"ALTER TABLE roof_face DROP COLUMN outline_json;
                   ALTER TABLE roof_face DROP COLUMN support_grid_json;
                   ALTER TABLE "array" DROP COLUMN layout_json;
                   ALTER TABLE "array" DROP COLUMN grid_block_cols;
                   ALTER TABLE "array" DROP COLUMN grid_block_rows;
                   ALTER TABLE "array" DROP COLUMN grid_block_extra_u;
                   ALTER TABLE "array" DROP COLUMN grid_block_extra_v;
                   ALTER TABLE "array" DROP COLUMN mounting_json;
                   ALTER TABLE building DROP COLUMN site_json;
                   ALTER TABLE project DROP COLUMN pricing_json;
                   DROP TABLE bom_snapshot;
                   DROP TABLE component_snapshot;
                   ALTER TABLE "array" DROP COLUMN module_ref;
                   ALTER TABLE "array" DROP COLUMN mounting_ref;
                   PRAGMA user_version = 1;"#,
            )
            .expect("downgrade to v1");
        }
        let reopened = ProjectStore::open(&path).expect("open v1");
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

        // Down-slope by 5 cm: the modules above are 2 cm away, moving up would collide.
        let mv = Command::MoveModules {
            ids: ids(&[0, 1]),
            du: 0.1,
            dv: -0.05,
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
    fn stress_presets_persist_and_encode() {
        let dir = tempfile::tempdir().expect("tmp");
        for (preset, count) in [
            (pv_model::StressPreset::Residential1k, 1008usize),
            (pv_model::StressPreset::Industrial10k, 10_080),
        ] {
            let path = dir.path().join(format!("{preset:?}.pvproj"));
            let created = ProjectStore::create(&path, preset.build()).expect("create");
            let original = created.project().clone();
            drop(created);
            let store = ProjectStore::open(&path).expect("open");
            assert_eq!(store.project(), &original);
            let buf = store.module_buffer();
            assert_eq!(buf.len(), buffer::HEADER_LEN + count * buffer::RECORD_LEN);
        }
    }

    /// The scripted benchmark (apps/desktop/src/perf/benchmark.ts) drags the first 100 active
    /// modules of the first array by DRAG_DU = 0.01 m along `u` and releases: that move
    /// must pass the layout validation, while 0.25 m (the former value) overlaps neighbours.
    #[test]
    fn benchmark_move_passes_layout_validation() {
        for preset in [
            pv_model::StressPreset::Residential1k,
            pv_model::StressPreset::Industrial10k,
        ] {
            let p = preset.build();
            let first = p.arrays[0].id;
            let picked: Vec<ModuleId> = p
                .modules
                .iter()
                .filter(|m| m.array_id == first && m.state == ModuleState::Active)
                .take(100)
                .map(|m| m.id)
                .collect();
            for (du, ok) in [(0.01, true), (0.25, false)] {
                let mut q = p.clone();
                Command::MoveModules {
                    ids: picked.clone(),
                    du,
                    dv: 0.0,
                }
                .apply(&mut q)
                .expect("pv-model accepts");
                let r = pv_layout::validate_module_positions(&q, &picked);
                assert_eq!(r.is_ok(), ok, "{preset:?} du = {du}: {r:?}");
            }
        }
    }

    /// pv-layout validation runs inside `apply`: a move onto a neighbour is rejected with
    /// `errors.layout.moduleOverlap`, the model is unchanged and nothing is journalled.
    #[test]
    fn colliding_move_is_rejected_by_layout_validation() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut s = new_store(&dir);
        let before = s.project().clone();
        // Module 0 (row 0, col 0) onto module 1 (row 0, col 1, 1.154 m to the right).
        let err = s
            .apply(
                &Command::MoveModules {
                    ids: ids(&[0]),
                    du: 0.6,
                    dv: 0.0,
                },
                Author::User,
            )
            .expect_err("overlap");
        assert!(
            matches!(err, StoreError::Command(ref e) if e.key == keys::LAYOUT_MODULE_OVERLAP),
            "{err:?}"
        );
        assert_eq!(s.project(), &before);
        assert_eq!(s.history_state().expect("h"), (false, false));
        // Removing modules is never blocked by the layout rules.
        s.apply(&Command::RemoveModules { ids: ids(&[0]) }, Author::User)
            .expect("remove");
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
