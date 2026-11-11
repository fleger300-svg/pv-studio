//! Prices: per piece, in cents, per supplier, over non-overlapping validity periods.

use pv_model::command::keys;
use pv_model::component::{select_price, Currency, IsoDate, PriceEntry};
use pv_model::{CommandError, ComponentId};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{LibraryError, LibraryStore};

fn currency(code: &str) -> Option<Currency> {
    match code {
        "CHF" => Some(Currency::Chf),
        "EUR" => Some(Currency::Eur),
        _ => None,
    }
}

impl LibraryStore {
    /// Sets the supplier whose prices win when several are valid (`None`: no preference).
    ///
    /// # Errors
    /// A database failure.
    pub fn set_preferred_supplier(&mut self, name: Option<&str>) -> Result<(), LibraryError> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE supplier SET preferred = 0 WHERE preferred = 1", [])?;
        if let Some(name) = name {
            tx.execute(
                "INSERT INTO supplier (name, preferred) VALUES (?1, 1)
                 ON CONFLICT(name) DO UPDATE SET preferred = 1",
                [name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// # Errors
    /// A database failure.
    pub fn preferred_supplier(&self) -> Result<Option<String>, LibraryError> {
        Ok(self
            .conn
            .query_row("SELECT name FROM supplier WHERE preferred = 1", [], |r| {
                r.get(0)
            })
            .optional()?)
    }

    /// Records a price. Periods of the same component and supplier never overlap: an earlier
    /// period still open on `valid_from` is closed the day before, a period starting the same
    /// day is replaced, and the new period ends before the next one starts.
    ///
    /// # Errors
    /// An unknown component, a negative price or an inverted period
    /// ([`keys::COMPONENT_INVALID`]), or a database failure.
    pub fn add_price(&mut self, entry: &PriceEntry) -> Result<(), LibraryError> {
        let Some(current) = self.get(entry.component)? else {
            return Err(CommandError::new(keys::COMPONENT_UNKNOWN)
                .with_text("id", entry.component.to_string())
                .into());
        };
        let invalid = |field: &str| -> LibraryError {
            current
                .component
                .error(keys::COMPONENT_INVALID)
                .with_text("field", field)
                .into()
        };
        if entry.list_price_cents < 0 {
            return Err(invalid("listPriceCents"));
        }
        if entry.supplier.trim().is_empty() {
            return Err(invalid("supplier"));
        }
        if entry
            .valid_to
            .as_ref()
            .is_some_and(|to| *to < entry.valid_from)
        {
            return Err(invalid("validTo"));
        }
        let id = entry.component.to_string();
        let from = entry.valid_from.as_str();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO supplier (name) VALUES (?1)",
            [&entry.supplier],
        )?;
        tx.execute(
            "DELETE FROM price WHERE component_id = ?1 AND supplier = ?2 AND valid_from = ?3",
            params![id, entry.supplier, from],
        )?;
        tx.execute(
            "UPDATE price SET valid_to = date(?3, '-1 day')
              WHERE component_id = ?1 AND supplier = ?2 AND valid_from < ?3
                AND (valid_to IS NULL OR valid_to >= ?3)",
            params![id, entry.supplier, from],
        )?;
        let next: Option<String> = tx.query_row(
            "SELECT date(MIN(valid_from), '-1 day') FROM price
              WHERE component_id = ?1 AND supplier = ?2 AND valid_from > ?3",
            params![id, entry.supplier, from],
            |r| r.get(0),
        )?;
        let to = match (
            entry.valid_to.as_ref().map(IsoDate::as_str),
            next.as_deref(),
        ) {
            (Some(to), Some(next)) => Some(to.min(next).to_owned()),
            (to, next) => to.or(next).map(str::to_owned),
        };
        tx.execute(
            "INSERT INTO price (component_id, supplier, list_price_cents, currency, valid_from, valid_to)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, entry.supplier, entry.list_price_cents, entry.currency.code(), from, to],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every price of `component`, oldest first.
    ///
    /// # Errors
    /// A database failure or corrupt rows.
    pub fn prices(&self, component: ComponentId) -> Result<Vec<PriceEntry>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT component_id, supplier, list_price_cents, currency, valid_from, valid_to
               FROM price WHERE component_id = ?1 ORDER BY valid_from, supplier",
        )?;
        let rows = stmt.query_map([component.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, supplier, cents, cur, from, to) = row?;
            let corrupt = |what: &str| LibraryError::Corrupt(format!("price {what}"));
            Ok(PriceEntry {
                component: ComponentId(Uuid::parse_str(&id).map_err(|_| corrupt("id"))?),
                supplier,
                list_price_cents: cents,
                currency: currency(&cur).ok_or_else(|| corrupt("currency"))?,
                valid_from: IsoDate::parse(&from).map_err(|_| corrupt("date"))?,
                valid_to: to
                    .map(|t| IsoDate::parse(&t))
                    .transpose()
                    .map_err(|_| corrupt("date"))?,
            })
        })
        .collect()
    }

    /// The price of `component` valid on `date`, at the preferred supplier when it has one.
    ///
    /// # Errors
    /// A database failure.
    pub fn price_on(
        &self,
        component: ComponentId,
        date: &IsoDate,
    ) -> Result<Option<PriceEntry>, LibraryError> {
        let entries = self.prices(component)?;
        let preferred = self.preferred_supplier()?;
        Ok(select_price(&entries, component, date, preferred.as_deref()).cloned())
    }
}
