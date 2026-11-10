//! Pricing commands: the discount of a manufacturer on the whole project.

use super::{keys, ChangeSet, Command, CommandError};
use crate::pricing::ManufacturerDiscount;
use crate::project::Project;

pub(super) fn inverse_discount(project: &Project, manufacturer: &str) -> Command {
    Command::SetDiscount {
        manufacturer: manufacturer.to_owned(),
        rate: project
            .pricing
            .discounts
            .iter()
            .find(|d| d.manufacturer == manufacturer)
            .map(|d| d.rate),
    }
}

pub(super) fn apply_discount(
    project: &mut Project,
    manufacturer: &str,
    rate: Option<f64>,
) -> Result<ChangeSet, CommandError> {
    if let Some(r) = rate {
        if !(r.is_finite() && (0.0..1.0).contains(&r)) {
            return Err(CommandError::new(keys::INVALID_DISCOUNT)
                .with_text("manufacturer", manufacturer)
                .with_number("rate", r));
        }
    }
    let discounts = &mut project.pricing.discounts;
    let current = discounts
        .iter()
        .position(|d| d.manufacturer == manufacturer);
    let changed = match (current, rate) {
        (Some(i), Some(r)) if discounts[i].rate == r => false,
        (Some(i), Some(r)) => {
            discounts[i].rate = r;
            true
        }
        (Some(i), None) => {
            discounts.remove(i);
            true
        }
        (None, Some(r)) => {
            discounts.push(ManufacturerDiscount {
                manufacturer: manufacturer.to_owned(),
                rate: r,
            });
            true
        }
        (None, None) => false,
    };
    Ok(ChangeSet {
        pricing: changed,
        ..ChangeSet::default()
    })
}
