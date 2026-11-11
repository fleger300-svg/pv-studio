//! Schema of `library.sqlite`, versioned with `PRAGMA user_version` (same scheme as
//! `pv-store`).
//!
//! Design (deviation from the architecture document, validated): a component is stored as
//! its typed JSON (`pv_model::Component`, versioned by `spec_version`), with indexed generated
//! columns for search, instead of one typed table per family. Descriptions live in the JSON:
//! there is no separate translation table, and no manufacturer table (the manufacturer is an
//! indexed generated column).

use rusqlite::Connection;

use crate::LibraryError;

/// Migration `i` upgrades the schema from version `i` to `i + 1`. Append only; never edit.
const MIGRATIONS: &[&str] = &[
    // v1: components, revisions, prices, suppliers, datasheets.
    r#"
    -- Current revision of every component. `data` is the full pv_model::Component JSON.
    CREATE TABLE component (
        id           TEXT PRIMARY KEY,
        revision     INTEGER NOT NULL CHECK (revision >= 1),
        spec_version INTEGER NOT NULL,
        data         TEXT NOT NULL CHECK (json_valid(data)),
        archived     INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
        updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        family       TEXT GENERATED ALWAYS AS (json_extract(data, '$.spec.family')) VIRTUAL,
        manufacturer TEXT GENERATED ALWAYS AS (json_extract(data, '$.manufacturer')) VIRTUAL,
        model        TEXT GENERATED ALWAYS AS (json_extract(data, '$.model')) VIRTUAL,
        article_no   TEXT GENERATED ALWAYS AS (json_extract(data, '$.articleNo')) VIRTUAL,
        label        TEXT GENERATED ALWAYS AS (json_extract(data, '$.label')) VIRTUAL,
        verified     INTEGER GENERATED ALWAYS AS (json_extract(data, '$.verified')) VIRTUAL,
        part_kind    TEXT GENERATED ALWAYS AS (json_extract(data, '$.spec.kind.kind')) VIRTUAL,
        pmax_w       REAL GENERATED ALWAYS AS (json_extract(data, '$.spec.electrical.pmaxW')) VIRTUAL,
        length_m     REAL GENERATED ALWAYS AS (json_extract(data, '$.spec.lengthM')) VIRTUAL,
        width_m      REAL GENERATED ALWAYS AS (json_extract(data, '$.spec.widthM')) VIRTUAL
    ) STRICT;
    CREATE INDEX component_family ON component(family, manufacturer, label);
    CREATE INDEX component_article ON component(manufacturer, article_no);
    CREATE INDEX component_pmax ON component(family, pmax_w);

    -- Every revision ever stored (audit of library changes; not part of any project journal).
    CREATE TABLE component_revision (
        component_id TEXT NOT NULL REFERENCES component(id),
        revision     INTEGER NOT NULL,
        spec_version INTEGER NOT NULL,
        data         TEXT NOT NULL CHECK (json_valid(data)),
        changed_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        author       TEXT NOT NULL CHECK (author IN ('user', 'import', 'seed', 'copilot')),
        note         TEXT NOT NULL DEFAULT '',
        PRIMARY KEY (component_id, revision)
    ) STRICT;

    CREATE TABLE supplier (
        name      TEXT PRIMARY KEY,
        preferred INTEGER NOT NULL DEFAULT 0 CHECK (preferred IN (0, 1))
    ) STRICT;
    CREATE UNIQUE INDEX supplier_one_preferred ON supplier(preferred) WHERE preferred = 1;

    -- List price of one piece, in cents. Periods of one (component, supplier) never overlap.
    CREATE TABLE price (
        id               INTEGER PRIMARY KEY,
        component_id     TEXT NOT NULL REFERENCES component(id),
        supplier         TEXT NOT NULL REFERENCES supplier(name),
        list_price_cents INTEGER NOT NULL CHECK (list_price_cents >= 0),
        currency         TEXT NOT NULL CHECK (currency IN ('CHF', 'EUR')),
        valid_from       TEXT NOT NULL,
        valid_to         TEXT CHECK (valid_to IS NULL OR valid_to >= valid_from),
        imported_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ) STRICT;
    CREATE INDEX price_component ON price(component_id, valid_from);

    -- Datasheets (PDF), deduplicated by content.
    CREATE TABLE blob (
        id         TEXT PRIMARY KEY,
        sha256     TEXT NOT NULL UNIQUE,
        media_type TEXT NOT NULL,
        size       INTEGER NOT NULL,
        bytes      BLOB NOT NULL
    ) STRICT;
    "#,
    // v2: where a component comes from (manual, import, AI extraction), for list badges.
    r#"
    ALTER TABLE component ADD COLUMN source TEXT
        GENERATED ALWAYS AS (json_extract(data, '$.source')) VIRTUAL;
    "#,
];

/// Latest schema version known to this build.
#[allow(clippy::cast_possible_wrap)]
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Sets connection pragmas and applies pending migrations.
///
/// # Errors
/// A newer schema, or a database failure.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
pub fn prepare(conn: &mut Connection) -> Result<(), LibraryError> {
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    // In-memory databases report "memory": accepted (tests).
    if !(mode.eq_ignore_ascii_case("wal") || mode.eq_ignore_ascii_case("memory")) {
        return Err(LibraryError::Corrupt(format!(
            "cannot enable WAL (got {mode})"
        )));
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(LibraryError::NewerSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        let violations: i64 =
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if violations > 0 {
            return Err(LibraryError::Corrupt(format!(
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
