//! Versioned migrations, tracked with `PRAGMA user_version`.

use rusqlite::Connection;

use crate::StoreError;

/// Migration `i` upgrades the schema from version `i` to `i + 1`. Append only; never edit.
const MIGRATIONS: &[&str] = &[
    // v1: initial schema.
    r#"
    CREATE TABLE project (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ) STRICT;

    CREATE TABLE building (
        id             TEXT PRIMARY KEY,
        project_id     TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
        ord            INTEGER NOT NULL,
        name           TEXT NOT NULL,
        origin_x       REAL NOT NULL,
        origin_y       REAL NOT NULL,
        origin_z       REAL NOT NULL,
        size_x         REAL NOT NULL,
        size_y         REAL NOT NULL,
        eave_height_m  REAL NOT NULL,
        ridge_height_m REAL NOT NULL,
        roof_kind      TEXT NOT NULL CHECK (roof_kind IN ('gable', 'flat'))
    ) STRICT;

    CREATE TABLE roof_face (
        id          TEXT PRIMARY KEY,
        building_id TEXT NOT NULL REFERENCES building(id) ON DELETE CASCADE,
        ord         INTEGER NOT NULL,
        name        TEXT NOT NULL,
        origin_x    REAL NOT NULL,
        origin_y    REAL NOT NULL,
        origin_z    REAL NOT NULL,
        tilt_deg    REAL NOT NULL,
        azimuth_deg REAL NOT NULL,
        width_m     REAL NOT NULL CHECK (width_m > 0),
        height_m    REAL NOT NULL CHECK (height_m > 0)
    ) STRICT;

    CREATE TABLE "array" (
        id                 TEXT PRIMARY KEY,
        roof_face_id       TEXT NOT NULL REFERENCES roof_face(id) ON DELETE CASCADE,
        ord                INTEGER NOT NULL UNIQUE,
        module_length_m    REAL NOT NULL,
        module_width_m     REAL NOT NULL,
        orientation        TEXT NOT NULL CHECK (orientation IN ('portrait', 'landscape')),
        tilt_on_roof_deg   REAL NOT NULL,
        azimuth_offset_deg REAL NOT NULL,
        grid_rows          INTEGER NOT NULL,
        grid_cols          INTEGER NOT NULL,
        grid_origin_u      REAL NOT NULL,
        grid_origin_v      REAL NOT NULL,
        grid_pitch_u       REAL NOT NULL,
        grid_pitch_v       REAL NOT NULL
    ) STRICT;

    -- id is the dense module id and the GPU instance index.
    CREATE TABLE module_instance (
        id           INTEGER PRIMARY KEY,
        array_id     TEXT NOT NULL REFERENCES "array"(id) ON DELETE CASCADE,
        grid_row     INTEGER NOT NULL,
        grid_col     INTEGER NOT NULL,
        u            REAL NOT NULL,
        v            REAL NOT NULL,
        state        TEXT NOT NULL CHECK (state IN ('active', 'removed')),
        string_index INTEGER NOT NULL
    ) STRICT;
    CREATE INDEX module_instance_array ON module_instance(array_id);

    CREATE TABLE command_log (
        seq     INTEGER PRIMARY KEY AUTOINCREMENT,
        at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        author  TEXT NOT NULL CHECK (author IN ('user', 'copilot', 'import')),
        command TEXT NOT NULL,
        inverse TEXT NOT NULL,
        undone  INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0, 1))
    ) STRICT;
    CREATE INDEX command_log_undone ON command_log(undone, seq);
    "#,
    // v2: layout data (pv-layout). Additive: existing rows keep working with the defaults.
    r#"
    ALTER TABLE roof_face ADD COLUMN outline_json TEXT;
    ALTER TABLE "array" ADD COLUMN layout_json TEXT NOT NULL DEFAULT '{}';
    ALTER TABLE "array" ADD COLUMN grid_block_cols INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE "array" ADD COLUMN grid_block_rows INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE "array" ADD COLUMN grid_block_extra_u REAL NOT NULL DEFAULT 0;
    ALTER TABLE "array" ADD COLUMN grid_block_extra_v REAL NOT NULL DEFAULT 0;
    "#,
    // v3: seams / rafters of the roof covering (pv-layout supports).
    r#"
    ALTER TABLE roof_face ADD COLUMN support_grid_json TEXT;
    "#,
    // v4: statics data (pv-pipeline): site per building, mounting snapshot per array, hipped
    // and monopitch roofs. SQLite cannot alter a CHECK constraint: the building table is
    // rebuilt (SQLite's 12-step procedure; foreign keys are off during migrations, see
    // `prepare`, so dropping the old table does not cascade to the roof faces).
    r#"
    CREATE TABLE building_v4 (
        id             TEXT PRIMARY KEY,
        project_id     TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
        ord            INTEGER NOT NULL,
        name           TEXT NOT NULL,
        origin_x       REAL NOT NULL,
        origin_y       REAL NOT NULL,
        origin_z       REAL NOT NULL,
        size_x         REAL NOT NULL,
        size_y         REAL NOT NULL,
        eave_height_m  REAL NOT NULL,
        ridge_height_m REAL NOT NULL,
        roof_kind      TEXT NOT NULL CHECK (roof_kind IN ('gable', 'flat', 'hipped', 'monopitch')),
        site_json      TEXT
    ) STRICT;
    INSERT INTO building_v4 (id, project_id, ord, name, origin_x, origin_y, origin_z, size_x, size_y,
                             eave_height_m, ridge_height_m, roof_kind)
        SELECT id, project_id, ord, name, origin_x, origin_y, origin_z, size_x, size_y,
               eave_height_m, ridge_height_m, roof_kind
        FROM building;
    DROP TABLE building;
    ALTER TABLE building_v4 RENAME TO building;
    ALTER TABLE "array" ADD COLUMN mounting_json TEXT;
    "#,
    // v5: mounting systems carry their parts (pv-bom); snapshots written before have none.
    r#"
    UPDATE "array"
       SET mounting_json = json_set(mounting_json, '$.parts', json('[]'))
     WHERE mounting_json IS NOT NULL AND json_extract(mounting_json, '$.parts') IS NULL;
    "#,
    // v6: project pricing (discount per manufacturer, set by commands) and frozen bills of
    // materials (immutable records: the basis of quotes and exports).
    r#"
    ALTER TABLE project ADD COLUMN pricing_json TEXT NOT NULL DEFAULT '{}';
    CREATE TABLE bom_snapshot (
        id                INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        label             TEXT NOT NULL,
        seq               INTEGER NOT NULL,
        input_fingerprint TEXT NOT NULL,
        pricing_json      TEXT NOT NULL,
        bom_json          TEXT NOT NULL
    ) STRICT;
    "#,
    // v7: footprint polygon and roof specification per building (pv-roof). Buildings written
    // before have an empty footprint: the loader rebuilds their rectangle from size_x/size_y.
    r#"
    ALTER TABLE building ADD COLUMN footprint_json TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE building ADD COLUMN roof_json TEXT;
    "#,
    // v8: component library. Arrays refer to frozen components (module and mounting system)
    // instead of carrying a hard-coded mounting snapshot. Old `mounting_json` snapshots are
    // converted on open by `pv_model::component::legacy` (content-derived ids, same resolved
    // system), then cleared; the undo journal of such files is cleared too (its commands
    // carry the old snapshot format).
    r#"
    CREATE TABLE component_snapshot (
        id             TEXT PRIMARY KEY,
        revision       INTEGER NOT NULL CHECK (revision >= 1),
        component_json TEXT NOT NULL CHECK (json_valid(component_json)),
        price_json     TEXT CHECK (price_json IS NULL OR json_valid(price_json))
    ) STRICT;
    ALTER TABLE "array" ADD COLUMN module_ref TEXT;
    ALTER TABLE "array" ADD COLUMN mounting_ref TEXT;
    "#,
];

/// Latest schema version known to this build.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Sets connection pragmas and applies pending migrations.
pub fn prepare(conn: &mut Connection) -> Result<(), StoreError> {
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::Corrupt(format!(
            "cannot enable WAL (got {mode})"
        )));
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    // Foreign keys off while migrating (table rebuilds must not cascade); checked before
    // each commit, then on for the session.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        let violations: i64 =
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if violations > 0 {
            return Err(StoreError::Corrupt(format!(
                "migration to v{} breaks {violations} foreign keys",
                i + 1
            )));
        }
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_on_fresh_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fresh.pvproj");
        let mut conn = Connection::open(&path).expect("open");
        prepare(&mut conn).expect("migrate");

        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .expect("v");
        assert_eq!(version, SCHEMA_VERSION);
        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .expect("fk");
        assert_eq!(fk, 1);
        let mode: String = conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .expect("jm");
        assert_eq!(mode, "wal");

        let mut tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .expect("stmt")
            .query_map([], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        tables.sort();
        assert_eq!(
            tables,
            [
                "array",
                "bom_snapshot",
                "building",
                "command_log",
                "component_snapshot",
                "module_instance",
                "project",
                "roof_face"
            ]
        );

        // Every table is STRICT.
        let non_strict: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_list WHERE schema = 'main' AND type = 'table' AND name NOT LIKE 'sqlite_%' AND strict = 0",
                [],
                |r| r.get(0),
            )
            .expect("table_list");
        assert_eq!(non_strict, 0, "all tables must be STRICT");
        // ...so a value that cannot be converted losslessly is rejected.
        let err = conn.execute(
            "INSERT INTO project (id, name, created_at) VALUES ('p', 'n', x'00')",
            [],
        );
        assert!(err.is_err(), "STRICT must reject a BLOB in a TEXT column");

        // Re-opening is idempotent.
        drop(conn);
        let mut conn = Connection::open(&path).expect("reopen");
        prepare(&mut conn).expect("migrate again");
    }

    #[test]
    fn author_check_constraint() {
        let mut conn = Connection::open_in_memory().expect("mem");
        // In-memory databases cannot use WAL; apply the schema directly for this check.
        conn.execute_batch(MIGRATIONS[0]).expect("schema");
        let tx = conn.transaction().expect("tx");
        let bad = tx.execute(
            "INSERT INTO command_log (author, command, inverse) VALUES ('robot', '{}', '{}')",
            [],
        );
        assert!(bad.is_err());
    }
}
