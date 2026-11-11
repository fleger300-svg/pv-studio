//! Search over the generated columns (no JSON parsing per row).

use pv_model::component::{ComponentFamily, ComponentSource};
use pv_model::ComponentId;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::{LibraryError, LibraryStore};

/// Filters of a library search; all optional.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    /// Words that must all appear in the manufacturer, model, article number or label
    /// (case-insensitive for ASCII).
    #[specta(optional)]
    pub text: Option<String>,
    #[specta(optional)]
    pub family: Option<ComponentFamily>,
    #[specta(optional)]
    pub manufacturer: Option<String>,
    /// Part kind tag (`rail`, `clamp`…), for mounting parts.
    #[specta(optional)]
    pub part_kind: Option<String>,
    #[specta(optional)]
    pub min_pmax_w: Option<f64>,
    #[specta(optional)]
    pub max_pmax_w: Option<f64>,
    pub include_archived: bool,
    /// Default 200.
    #[specta(optional)]
    pub limit: Option<u32>,
}

/// One row of a search result (no spec: lists stay light).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSummary {
    pub id: ComponentId,
    pub revision: u32,
    pub family: ComponentFamily,
    pub manufacturer: String,
    pub model: String,
    pub article_no: String,
    pub label: String,
    pub verified: bool,
    pub source: ComponentSource,
    pub archived: bool,
    #[specta(optional)]
    pub part_kind: Option<String>,
    #[specta(optional)]
    pub pmax_w: Option<f64>,
    #[specta(optional)]
    pub length_m: Option<f64>,
    #[specta(optional)]
    pub width_m: Option<f64>,
}

fn family_of(tag: &str) -> Option<ComponentFamily> {
    Some(match tag {
        "module" => ComponentFamily::Module,
        "mountingSystem" => ComponentFamily::MountingSystem,
        "mountingPart" => ComponentFamily::MountingPart,
        _ => return None,
    })
}

impl LibraryStore {
    /// Components matching `query`, by manufacturer then label.
    ///
    /// # Errors
    /// A database failure.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<ComponentSummary>, LibraryError> {
        let mut sql = String::from(
            "SELECT id, revision, family, manufacturer, model, article_no, label, verified,
                    archived, part_kind, pmax_w, length_m, width_m, source
               FROM component WHERE 1 = 1",
        );
        let mut args: Vec<Value> = Vec::new();
        let mut push = |clause: &str, v: Value, sql: &mut String| {
            args.push(v);
            sql.push_str(&clause.replace('?', &format!("?{}", args.len())));
        };
        if !query.include_archived {
            sql.push_str(" AND archived = 0");
        }
        if let Some(f) = query.family {
            push(" AND family = ?", Value::Text(f.as_str().into()), &mut sql);
        }
        if let Some(m) = &query.manufacturer {
            push(" AND manufacturer = ?", Value::Text(m.clone()), &mut sql);
        }
        if let Some(k) = &query.part_kind {
            push(" AND part_kind = ?", Value::Text(k.clone()), &mut sql);
        }
        if let Some(p) = query.min_pmax_w {
            push(" AND pmax_w >= ?", Value::Real(p), &mut sql);
        }
        if let Some(p) = query.max_pmax_w {
            push(" AND pmax_w <= ?", Value::Real(p), &mut sql);
        }
        for word in query.text.iter().flat_map(|t| t.split_whitespace()) {
            let pattern = format!(
                "%{}%",
                word.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            let n = args.len() + 1;
            args.push(Value::Text(pattern));
            sql.push_str(&format!(
                " AND (manufacturer LIKE ?{n} ESCAPE '\\' OR model LIKE ?{n} ESCAPE '\\'
                       OR article_no LIKE ?{n} ESCAPE '\\' OR label LIKE ?{n} ESCAPE '\\')"
            ));
        }
        sql.push_str(" ORDER BY manufacturer, label, id");
        sql.push_str(&format!(" LIMIT {}", query.limit.unwrap_or(200)));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
            let id: String = r.get(0)?;
            let family: String = r.get(2)?;
            Ok((
                id,
                family,
                ComponentSummary {
                    id: ComponentId(Uuid::nil()),
                    revision: r.get(1)?,
                    family: ComponentFamily::Module,
                    manufacturer: r.get(3)?,
                    model: r.get(4)?,
                    article_no: r.get(5)?,
                    label: r.get(6)?,
                    verified: r.get::<_, Option<bool>>(7)?.unwrap_or(false),
                    archived: r.get(8)?,
                    part_kind: r.get(9)?,
                    pmax_w: r.get(10)?,
                    length_m: r.get(11)?,
                    width_m: r.get(12)?,
                    source: match r.get::<_, Option<String>>(13)?.as_deref() {
                        Some("import") => ComponentSource::Import,
                        Some("aiExtracted") => ComponentSource::AiExtracted,
                        _ => ComponentSource::Manual,
                    },
                },
            ))
        })?;
        rows.map(|row| {
            let (id, family, mut summary) = row?;
            summary.id = ComponentId(
                Uuid::parse_str(&id).map_err(|e| LibraryError::Corrupt(e.to_string()))?,
            );
            summary.family = family_of(&family)
                .ok_or_else(|| LibraryError::Corrupt(format!("unknown family {family}")))?;
            Ok(summary)
        })
        .collect()
    }
}
