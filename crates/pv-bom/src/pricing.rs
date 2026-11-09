//! Sale units and prices (ADR-005), in integer cents.
//!
//! - Sale units are computed once on a quantity: per tab on that tab's quantity, and on the
//!   project total for the Total tab (128 rails → 7 packs of 20, not the 8 that adding the
//!   per-face packs would give).
//! - Unit price = list price × (1 − manufacturer discount), rounded to the cent; price =
//!   pieces × unit price (as K2: on pieces, not on packs).

use pv_model::Article;

use crate::error::BomError;

/// Value in cents (prices are given to the cent).
#[must_use]
pub fn cents(value: f64) -> i64 {
    (value * 100.0).round() as i64
}

/// Sale units of `quantity` pieces.
///
/// # Errors
/// [`BomError::ZeroPack`].
pub fn sale_units(article: &Article, quantity: u64) -> Result<u64, BomError> {
    match u64::from(article.pack_qty) {
        0 => Err(BomError::ZeroPack {
            article_no: article.article_no.clone(),
        }),
        pack => Ok(quantity.div_ceil(pack)),
    }
}

/// Unit price after discount, in cents.
///
/// # Errors
/// [`BomError::InvalidDiscount`] for a rate outside `[0, 1)`.
pub fn unit_price_cents(article: &Article, rate: f64) -> Result<i64, BomError> {
    if !(rate.is_finite() && (0.0..1.0).contains(&rate)) {
        return Err(BomError::InvalidDiscount {
            manufacturer: article.manufacturer.clone(),
            rate,
        });
    }
    Ok((cents(article.list_price) as f64 * (1.0 - rate)).round() as i64)
}
