//! Articles (mounting parts, modules…) as snapshotted into a project.
//!
//! Architecture: « Systèmes, pièces (n° d'article, poids, conditionnement) ». Descriptions are
//! translated when the library provides them; otherwise the manufacturer's label is shown
//! (« repli sur le libellé du fabricant si la traduction manque »).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

/// One article of a manufacturer's catalogue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    /// Manufacturer's article number (e.g. `2004393`).
    pub article_no: String,
    pub manufacturer: String,
    /// Manufacturer's label, shown when no translation exists.
    pub label: String,
    /// Translated descriptions by language (`fr`, `de`, `en`); may be empty.
    pub descriptions: BTreeMap<String, String>,
    /// Mass of one piece [kg].
    pub unit_mass_kg: f64,
    /// Pieces per sale unit (pack, bundle).
    pub pack_qty: u32,
    /// List price of one piece, in `currency`; 0 when unknown.
    pub list_price: f64,
    pub currency: String,
}

impl Article {
    /// Description in `lang`, or the manufacturer's label.
    #[must_use]
    pub fn description(&self, lang: &str) -> &str {
        self.descriptions
            .get(lang)
            .map_or(&self.label, String::as_str)
    }
}
