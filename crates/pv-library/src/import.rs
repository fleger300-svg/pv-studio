//! Price list import (K2 Base material lists, CSV), from raw sheet rows.
//!
//! Deviation from the plan, validated by the internal review: the workbook is read by the
//! front end (exceljs in a Web Worker, already used for the K2 export), which sends raw cell
//! values; everything that matters lives here — column recognition (French, German and
//! English headers), matching with the library, validation, cents, periods and the report.
//! No new native dependency, and the tests run on rows.
//!
//! Nothing changes silently: an import only adds prices (new periods) and, when asked,
//! unverified parts for unknown articles. Differences with the library's data (pack quantity,
//! label) are reported, never applied.

use std::collections::BTreeMap;

use pv_model::command::keys;
use pv_model::component::{
    ComponentFamily, ComponentSource, Currency, Descriptions, IsoDate, PartKind, PartSpec,
    PriceEntry,
};
use pv_model::{CommandError, Component, ComponentId, ComponentSpec};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ChangeAuthor, LibraryError, LibraryStore, SearchQuery};

/// One cell as read from the workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum ImportCell {
    Empty,
    Text(String),
    Number(f64),
}

impl ImportCell {
    fn text(&self) -> Option<String> {
        match self {
            Self::Text(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
            // Article numbers are often stored as numbers.
            #[allow(clippy::cast_possible_truncation)]
            Self::Number(n) if n.fract() == 0.0 => Some(format!("{}", *n as i64)),
            _ => None,
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            Self::Number(n) if n.is_finite() => Some(*n),
            // "57.09", "57,09", "CHF 1'234.50".
            Self::Text(s) => {
                let cleaned: String = s
                    .chars()
                    .filter(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-'))
                    .collect();
                cleaned.replace(',', ".").parse().ok()
            }
            _ => None,
        }
    }
}

/// One sheet of the workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSheet {
    pub name: String,
    pub rows: Vec<Vec<ImportCell>>,
}

/// What to import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    /// Manufacturer of the articles (K2 lists: `K2 Systems`).
    pub manufacturer: String,
    /// Supplier the prices are from.
    pub supplier: String,
    pub currency: Currency,
    /// First day of validity of the imported prices.
    pub valid_from: IsoDate,
    /// Create an unverified part for each unknown article (else they are only reported).
    pub create_missing: bool,
    pub sheets: Vec<ImportSheet>,
}

/// A difference between the list and the library, reported (never applied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportDifference {
    pub article_no: String,
    /// `packQty` or `label`.
    pub field: String,
    pub library: String,
    pub list: String,
}

/// Outcome of an import (or of its preview).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Articles priced (existing or created).
    pub priced: Vec<String>,
    /// Articles created as unverified parts.
    pub created: Vec<String>,
    /// Articles not in the library and not created.
    pub unknown: Vec<String>,
    /// Articles listed with different prices in different sheets (not imported).
    pub conflicts: Vec<String>,
    pub differences: Vec<ImportDifference>,
    /// Sheets without recognisable headers.
    pub skipped_sheets: Vec<String>,
}

/// Columns of a sheet.
struct Columns {
    article: usize,
    label: Option<usize>,
    pack: Option<usize>,
    price: usize,
}

fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            'à' | 'â' => 'a',
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            c => c,
        })
        .collect()
}

/// Recognises the header row (K2 Base in French, German or English; generic CSV names).
fn columns(header: &[ImportCell]) -> Option<Columns> {
    let names: Vec<String> = header
        .iter()
        .map(|c| c.text().map(|t| normalise(&t)).unwrap_or_default())
        .collect();
    let find = |pred: &dyn Fn(&str) -> bool| names.iter().position(|n| pred(n));
    let article = find(&|n| {
        (n.starts_with("art") && (n.contains("no") || n.contains("nr") || n.contains("num")))
            || n == "article"
            || n == "sku"
    })?;
    let price = find(&|n| {
        n.contains("prix catalogue")
            || n.contains("listenpreis")
            || n.contains("list price")
            || n == "price"
            || n == "prix"
    })?;
    let label = find(&|n| {
        n.contains("description") || n.contains("bezeichnung") || n.contains("beschreibung")
    });
    let pack = find(&|n| {
        n.contains("par unite de vente")
            || n.contains("pro verkaufseinheit")
            || n.contains("per sales unit")
            || n.contains("per sale unit")
            || n == "pack"
    });
    Some(Columns {
        article,
        label,
        pack,
        price,
    })
}

/// One article as listed.
#[derive(Debug, Clone, PartialEq)]
struct Listed {
    label: Option<String>,
    pack: Option<u32>,
    cents: i64,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn listed(row: &[ImportCell], c: &Columns) -> Option<(String, Listed)> {
    let cell = |i: usize| row.get(i).unwrap_or(&ImportCell::Empty);
    let article = cell(c.article).text()?;
    let price = cell(c.price).number().filter(|p| *p >= 0.0)?;
    Some((
        article,
        Listed {
            label: c.label.and_then(|i| cell(i).text()),
            pack: c
                .pack
                .and_then(|i| cell(i).number())
                .filter(|p| *p >= 1.0 && p.fract() == 0.0)
                .map(|p| p as u32),
            cents: (price * 100.0).round() as i64,
        },
    ))
}

/// What an import will do.
struct Plan {
    report: ImportReport,
    prices: Vec<(ComponentId, i64)>,
    create: Vec<Component>,
}

impl LibraryStore {
    /// Part components of `manufacturer` by article number.
    fn parts_by_article(
        &self,
        manufacturer: &str,
    ) -> Result<BTreeMap<String, Component>, LibraryError> {
        let summaries = self.search(&SearchQuery {
            family: Some(ComponentFamily::MountingPart),
            manufacturer: Some(manufacturer.to_owned()),
            include_archived: true,
            limit: Some(1_000_000),
            ..SearchQuery::default()
        })?;
        let ids: Vec<ComponentId> = summaries.iter().map(|s| s.id).collect();
        Ok(self
            .load_all(&ids)?
            .into_iter()
            .filter(|c| !c.article_no.is_empty())
            .map(|c| (c.article_no.clone(), c))
            .collect())
    }

    fn plan_import(&self, request: &ImportRequest) -> Result<Plan, LibraryError> {
        if request.supplier.trim().is_empty() || request.manufacturer.trim().is_empty() {
            return Err(CommandError::new(keys::COMPONENT_INVALID)
                .with_text("component", "import")
                .with_text("field", "supplier")
                .into());
        }
        let mut report = ImportReport::default();
        let mut articles: BTreeMap<String, Listed> = BTreeMap::new();
        for sheet in &request.sheets {
            let Some((h, cols)) = sheet
                .rows
                .iter()
                .enumerate()
                .take(20)
                .find_map(|(i, r)| columns(r).map(|c| (i, c)))
            else {
                report.skipped_sheets.push(sheet.name.clone());
                continue;
            };
            for row in &sheet.rows[h + 1..] {
                let Some((no, item)) = listed(row, &cols) else {
                    continue;
                };
                match articles.get(&no) {
                    Some(prev) if prev.cents != item.cents => {
                        if !report.conflicts.contains(&no) {
                            report.conflicts.push(no.clone());
                        }
                    }
                    Some(_) => {}
                    None => {
                        articles.insert(no, item);
                    }
                }
            }
        }
        let known = self.parts_by_article(&request.manufacturer)?;
        let mut prices = Vec::new();
        let mut create = Vec::new();
        for (no, item) in articles {
            if report.conflicts.contains(&no) {
                continue;
            }
            if let Some(c) = known.get(&no) {
                if let (Some(pack), ComponentSpec::MountingPart(p)) = (item.pack, &c.spec) {
                    if pack != p.pack_qty {
                        report.differences.push(ImportDifference {
                            article_no: no.clone(),
                            field: "packQty".into(),
                            library: p.pack_qty.to_string(),
                            list: pack.to_string(),
                        });
                    }
                }
                if let Some(label) = item.label.as_ref().filter(|l| **l != c.label) {
                    report.differences.push(ImportDifference {
                        article_no: no.clone(),
                        field: "label".into(),
                        library: c.label.clone(),
                        list: label.clone(),
                    });
                }
                prices.push((c.id, item.cents));
                report.priced.push(no);
            } else if request.create_missing {
                let label = item.label.clone().unwrap_or_else(|| no.clone());
                let part = Component {
                    id: ComponentId::new(),
                    revision: 1,
                    manufacturer: request.manufacturer.clone(),
                    model: label.clone(),
                    article_no: no.clone(),
                    label,
                    descriptions: Descriptions::default(),
                    source: ComponentSource::Import,
                    verified: false,
                    datasheet: None,
                    spec: ComponentSpec::MountingPart(PartSpec {
                        unit_mass_kg: 0.0,
                        pack_qty: item.pack.unwrap_or(1),
                        kind: PartKind::Other,
                    }),
                };
                prices.push((part.id, item.cents));
                create.push(part);
                report.created.push(no.clone());
                report.priced.push(no);
            } else {
                report.unknown.push(no);
            }
        }
        report.priced.sort();
        Ok(Plan {
            report,
            prices,
            create,
        })
    }

    /// What [`Self::import_prices`] would do, without changing anything.
    ///
    /// # Errors
    /// No supplier or manufacturer, or a database failure.
    pub fn preview_import(&self, request: &ImportRequest) -> Result<ImportReport, LibraryError> {
        Ok(self.plan_import(request)?.report)
    }

    /// Imports the list's prices (and, when asked, creates unverified parts for unknown
    /// articles).
    ///
    /// # Errors
    /// No supplier or manufacturer, or a database failure.
    pub fn import_prices(&mut self, request: &ImportRequest) -> Result<ImportReport, LibraryError> {
        let plan = self.plan_import(request)?;
        for part in plan.create {
            self.create(part, ChangeAuthor::Import, "price list import")?;
        }
        for (component, cents) in plan.prices {
            self.add_price(&PriceEntry {
                component,
                supplier: request.supplier.clone(),
                list_price_cents: cents,
                currency: request.currency,
                valid_from: request.valid_from.clone(),
                valid_to: None,
            })?;
        }
        Ok(plan.report)
    }
}
