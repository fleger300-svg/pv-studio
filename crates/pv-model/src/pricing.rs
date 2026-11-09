//! Commercial conditions of a project: one discount per manufacturer, on the whole quote
//! (the installers' usual terms, e.g. −35 % on all K2 material).

use serde::{Deserialize, Serialize};
use specta::Type;

/// Discount granted by a manufacturer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturerDiscount {
    pub manufacturer: String,
    /// Fraction of the list price, in `[0, 1)` (0.35 = −35 %).
    pub rate: f64,
}

/// Pricing of a project.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Pricing {
    pub discounts: Vec<ManufacturerDiscount>,
}

impl Pricing {
    /// Discount rate of `manufacturer` (0 when none is set).
    #[must_use]
    pub fn rate_for(&self, manufacturer: &str) -> f64 {
        self.discounts
            .iter()
            .find(|d| d.manufacturer == manufacturer)
            .map_or(0.0, |d| d.rate)
    }
}
