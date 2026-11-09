//! The bill of materials as K2 prints it: one table per face and a Total, with the K2
//! columns. This is what the UI shows and the Excel export writes.

use std::collections::BTreeMap;

use serde::Serialize;
use specta::Type;

/// One article line (K2 columns).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BomLine {
    pub article_no: String,
    pub manufacturer: String,
    /// Manufacturer's label (shown when `descriptions` lacks the UI language).
    pub label: String,
    pub descriptions: BTreeMap<String, String>,
    /// Pieces.
    pub quantity: u32,
    /// quantity × unit mass [kg].
    pub mass_kg: f64,
    /// Packs: ⌈quantity / pack⌉ on this table's quantity (ADR-005).
    pub sale_units: u32,
    pub pack_qty: u32,
    pub list_price: f64,
    /// Manufacturer discount of the project, in `[0, 1)`.
    pub discount_rate: f64,
    /// List price after discount, to the cent.
    pub unit_price: f64,
    /// quantity × unit price.
    pub price: f64,
}

/// One tab: a roof face, or the project total.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BomTable {
    /// `None` for the Total.
    pub face_id: Option<String>,
    pub title: String,
    pub lines: Vec<BomLine>,
    pub mass_kg: f64,
    pub price: f64,
}

/// Bars saved on the project by reusing offcuts across faces: the face tabs need
/// `per_faces` bars (each face cut alone), the Total `project`.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedBars {
    pub article_no: String,
    pub per_faces: u32,
    pub project: u32,
}

/// The bill of materials of a project.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Bom {
    pub faces: Vec<BomTable>,
    pub total: BomTable,
    /// Only the articles where the project-wide cut saves bars.
    pub saved_bars: Vec<SavedBars>,
}
