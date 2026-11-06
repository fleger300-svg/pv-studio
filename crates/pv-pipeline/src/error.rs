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
    match e {
        LayoutError::Outline(_)
        | LayoutError::EdgeKindCount { .. }
        | LayoutError::ObstacleOffRoof { .. } => CommandError::new(keys::LAYOUT_INVALID_OUTLINE),
        other => CommandError::new(keys::LAYOUT_INVALID_PARAMETERS)
            .with_text("detail", other.to_string()),
    }
}
