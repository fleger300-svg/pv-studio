//! Errors of the bill of materials (i18n keys, never prose).

use pv_model::command::keys;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BomError {
    #[error("article {article_no}: pack quantity is 0")]
    ZeroPack { article_no: String },
    #[error("article {article_no}: invalid rail piece length {length_m} m")]
    InvalidPiece { article_no: String, length_m: f64 },
    #[error("article {article_no}: invalid bar length {stock_m} m or saw cut {kerf_m} m")]
    InvalidStock {
        article_no: String,
        stock_m: f64,
        kerf_m: f64,
    },
    #[error("discount {rate} of {manufacturer} is outside [0, 1)")]
    InvalidDiscount { manufacturer: String, rate: f64 },
}

impl BomError {
    /// Translation key of the error.
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            BomError::ZeroPack { .. } => keys::BOM_ZERO_PACK,
            BomError::InvalidPiece { .. } => keys::BOM_INVALID_PIECE,
            BomError::InvalidStock { .. } => keys::BOM_INVALID_STOCK,
            BomError::InvalidDiscount { .. } => keys::BOM_INVALID_DISCOUNT,
        }
    }
}
