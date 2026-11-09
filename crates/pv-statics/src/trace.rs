//! Traceability (ADR-004).
//!
//! Every number the engine produces is a [`Traced`] value: it carries its symbol, unit, the
//! formula that produced it, the clause of the standard it comes from, the inputs that went
//! into it and the assumptions it depends on. Reports and the UI render these directly, so a
//! pre-dimensioning result can always be explained and checked by hand.

use serde::Serialize;

/// Units used by the statics engine. Loads are in kN and kN/m², lengths in m.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Dimensionless,
    Metre,
    SquareMetre,
    Degree,
    Year,
    Kilogram,
    KilogramPerSquareMetre,
    KiloNewton,
    KiloNewtonPerSquareMetre,
}

impl Unit {
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Dimensionless => "",
            Unit::Metre => "m",
            Unit::SquareMetre => "m²",
            Unit::Degree => "°",
            Unit::Year => "a",
            Unit::Kilogram => "kg",
            Unit::KilogramPerSquareMetre => "kg/m²",
            Unit::KiloNewton => "kN",
            Unit::KiloNewtonPerSquareMetre => "kN/m²",
        }
    }
}

/// Reference to the document and clause a formula or value comes from.
///
/// References to SIA / EN standards are never translated (see the i18n rules), so they are
/// plain strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NormRef {
    pub document: &'static str,
    pub clause: &'static str,
}

impl NormRef {
    #[must_use]
    pub const fn new(document: &'static str, clause: &'static str) -> Self {
        Self { document, clause }
    }
}

/// One named input of a formula, as it was evaluated.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Input {
    pub symbol: String,
    pub value: f64,
    pub unit: Unit,
}

/// A hypothesis the result depends on. `key` is a message key resolved by the i18n layer;
/// `detail` is a language-neutral English fallback that also carries the actual values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assumption {
    pub key: &'static str,
    pub detail: String,
}

/// A computed value together with everything needed to explain it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Traced {
    pub symbol: String,
    pub value: f64,
    pub unit: Unit,
    /// Formula in plain text (Unicode maths), e.g. `s_k = max(0.4·[1 + (h₀/350)²], 0.9)`.
    pub formula: String,
    pub reference: Option<NormRef>,
    pub inputs: Vec<Input>,
    pub assumptions: Vec<Assumption>,
}

impl Traced {
    #[must_use]
    pub fn new(symbol: impl Into<String>, value: f64, unit: Unit) -> Self {
        Self {
            symbol: symbol.into(),
            value,
            unit,
            formula: String::new(),
            reference: None,
            inputs: Vec::new(),
            assumptions: Vec::new(),
        }
    }

    /// A value given by the user or by a data source, not computed.
    #[must_use]
    pub fn given(symbol: impl Into<String>, value: f64, unit: Unit) -> Self {
        Self::new(symbol, value, unit).formula("given")
    }

    #[must_use]
    pub fn formula(mut self, formula: impl Into<String>) -> Self {
        self.formula = formula.into();
        self
    }

    #[must_use]
    pub fn reference(mut self, reference: NormRef) -> Self {
        self.reference = Some(reference);
        self
    }

    #[must_use]
    pub fn input(mut self, symbol: impl Into<String>, value: f64, unit: Unit) -> Self {
        self.inputs.push(Input {
            symbol: symbol.into(),
            value,
            unit,
        });
        self
    }

    /// Record another traced value as an input of this one.
    #[must_use]
    pub fn input_from(self, other: &Traced) -> Self {
        let (symbol, value, unit) = (other.symbol.clone(), other.value, other.unit);
        self.input(symbol, value, unit)
    }

    #[must_use]
    pub fn assume(mut self, key: &'static str, detail: impl Into<String>) -> Self {
        self.assumptions.push(Assumption {
            key,
            detail: detail.into(),
        });
        self
    }

    /// Human-readable one-liner, e.g. `s_k = 0.900 kN/m²  [SIA 261 §5.2]`.
    #[must_use]
    pub fn display(&self) -> String {
        let unit = self.unit.symbol();
        let sep = if unit.is_empty() { "" } else { " " };
        let reference = self
            .reference
            .map(|r| format!("  [{} {}]", r.document, r.clause))
            .unwrap_or_default();
        format!("{} = {:.3}{sep}{unit}{reference}", self.symbol, self.value)
    }
}

/// Stable 64-bit fingerprint (FNV-1a) of any serialisable input. It lives in `pv-model` so
/// that the dependency graph (`pv-graph`) shares it; re-exported here unchanged.
pub use pv_model::fingerprint::fingerprint;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_unit_and_reference() {
        let t = Traced::new("s_k", 0.9, Unit::KiloNewtonPerSquareMetre)
            .reference(NormRef::new("SIA 261", "§5.2"));
        assert_eq!(t.display(), "s_k = 0.900 kN/m²  [SIA 261 §5.2]");
    }
}
