//! List prices, in integer cents, with supplier and validity period.

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::command::{keys, CommandError};
use crate::ids::ComponentId;

/// Currency of a price (ISO 4217).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    #[default]
    Chf,
    Eur,
}

impl Currency {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Chf => "CHF",
            Self::Eur => "EUR",
        }
    }
}

/// Calendar date, serialised as `YYYY-MM-DD` (so string order is date order).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(try_from = "String", into = "String")]
pub struct IsoDate(String);

impl IsoDate {
    /// # Errors
    /// [`keys::COMPONENT_INVALID_DATE`] unless `value` is a real date written `YYYY-MM-DD`.
    pub fn parse(value: &str) -> Result<Self, CommandError> {
        let invalid = || CommandError::new(keys::COMPONENT_INVALID_DATE).with_text("value", value);
        let b = value.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return Err(invalid());
        }
        let num = |s: &str| {
            s.bytes()
                .all(|c| c.is_ascii_digit())
                .then(|| s.parse::<u32>().ok())
                .flatten()
        };
        let (Some(y), Some(m), Some(d)) = (num(&value[..4]), num(&value[5..7]), num(&value[8..]))
        else {
            return Err(invalid());
        };
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let days = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err(invalid()),
        };
        if (1..=days).contains(&d) {
            Ok(Self(value.to_owned()))
        } else {
            Err(invalid())
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for IsoDate {
    type Error = CommandError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<IsoDate> for String {
    fn from(date: IsoDate) -> Self {
        date.0
    }
}

impl fmt::Display for IsoDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// List price of one piece of a component at a supplier, valid over a period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceEntry {
    pub component: ComponentId,
    pub supplier: String,
    /// Per piece, not per pack (discounts and sale units are applied by the BOM engine).
    /// Exported to TypeScript as `number` (exact below 2⁵³ cents).
    #[specta(type = f64)]
    pub list_price_cents: i64,
    pub currency: Currency,
    /// First day of validity (inclusive).
    pub valid_from: IsoDate,
    /// Last day of validity (inclusive); `None` = open-ended.
    #[specta(optional)]
    pub valid_to: Option<IsoDate>,
}

impl PriceEntry {
    #[must_use]
    pub fn is_valid_on(&self, date: &IsoDate) -> bool {
        self.valid_from <= *date && self.valid_to.as_ref().is_none_or(|to| date <= to)
    }

    /// List price in currency units, as the engines' [`crate::Article`] expects it.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // cents well below 2⁵³
    pub fn list_price(&self) -> f64 {
        self.list_price_cents as f64 / 100.0
    }
}

/// The price of `component` on `date`: at the `preferred` supplier when it has one, else at
/// any supplier; among candidates, the most recent `valid_from`, then the supplier name
/// (deterministic).
#[must_use]
pub fn select_price<'a>(
    entries: &'a [PriceEntry],
    component: ComponentId,
    date: &IsoDate,
    preferred: Option<&str>,
) -> Option<&'a PriceEntry> {
    entries
        .iter()
        .filter(|e| e.component == component && e.is_valid_on(date))
        .max_by(|a, b| {
            let pref = |e: &PriceEntry| preferred == Some(e.supplier.as_str());
            pref(a)
                .cmp(&pref(b))
                .then_with(|| a.valid_from.cmp(&b.valid_from))
                .then_with(|| b.supplier.cmp(&a.supplier))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> IsoDate {
        IsoDate::parse(s).expect("date")
    }

    #[test]
    fn dates_are_checked() {
        for ok in ["2026-11-11", "2024-02-29", "2000-02-29"] {
            assert!(IsoDate::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "2026-1-11",
            "2026-13-01",
            "2026-02-29",
            "1900-02-29",
            "2026-04-31",
            "2026/11/11",
            "+026-11-11",
        ] {
            assert!(IsoDate::parse(bad).is_err(), "{bad}");
        }
        let json = serde_json::to_string(&date("2026-11-11")).expect("ser");
        assert_eq!(json, r#""2026-11-11""#);
        assert!(serde_json::from_str::<IsoDate>(r#""2026-02-30""#).is_err());
    }

    #[test]
    fn price_selection() {
        let c = ComponentId::new();
        let entry = |supplier: &str, cents, from: &str, to: Option<&str>| PriceEntry {
            component: c,
            supplier: supplier.into(),
            list_price_cents: cents,
            currency: Currency::Chf,
            valid_from: date(from),
            valid_to: to.map(date),
        };
        let entries = [
            entry("A", 100, "2025-01-01", Some("2025-12-31")),
            entry("A", 110, "2026-01-01", None),
            entry("B", 90, "2026-06-01", None),
            entry("C", 80, "2026-06-01", None),
        ];
        let pick =
            |d: &str, pref| select_price(&entries, c, &date(d), pref).map(|e| e.list_price_cents);
        assert_eq!(pick("2025-12-31", None), Some(100), "valid_to is inclusive");
        assert_eq!(pick("2026-03-01", None), Some(110));
        assert_eq!(
            pick("2026-07-01", None),
            Some(90),
            "most recent, then supplier name"
        );
        assert_eq!(
            pick("2026-07-01", Some("A")),
            Some(110),
            "preferred supplier wins"
        );
        assert_eq!(pick("2024-01-01", None), None);
        assert_eq!(
            select_price(&entries, ComponentId::new(), &date("2026-07-01"), None),
            None
        );
    }
}
