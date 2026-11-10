//! Engine errors as i18n `CommandError`s (never prose).

use pv_layout::LayoutError;
use pv_model::command::keys;
use pv_model::CommandError;
use pv_statics::StaticsError;

use crate::convert::wind_zone;

#[must_use]
pub fn statics_error(e: &StaticsError) -> CommandError {
    match e {
        StaticsError::OutOfRange {
            quantity,
            value,
            min,
            max,
        } => CommandError::new(keys::STATICS_OUT_OF_RANGE)
            .with_text("quantity", *quantity)
            .with_number("value", *value)
            .with_number("min", *min)
            .with_number("max", *max),
        StaticsError::MissingCoefficient { zone } => {
            CommandError::new(keys::STATICS_MISSING_COEFFICIENT)
                .with_text("zone", format!("{:?}", wind_zone(*zone)))
        }
        StaticsError::Unsupported { what } => {
            CommandError::new(keys::STATICS_UNSUPPORTED).with_text("what", what)
        }
    }
}

#[must_use]
pub fn layout_error(e: &LayoutError) -> CommandError {
    e.command_error()
}

#[must_use]
pub fn bom_error(e: &pv_bom::BomError) -> CommandError {
    use pv_bom::BomError;
    let error = CommandError::new(e.key());
    match e {
        BomError::ZeroPack { article_no } | BomError::InvalidStock { article_no, .. } => {
            error.with_text("articleNo", article_no.clone())
        }
        BomError::InvalidPiece {
            article_no,
            length_m,
        } => error
            .with_text("articleNo", article_no.clone())
            .with_number("length", *length_m),
        BomError::InvalidDiscount { manufacturer, rate } => error
            .with_text("manufacturer", manufacturer.clone())
            .with_number("rate", *rate),
    }
}
