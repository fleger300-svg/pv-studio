//! Rail pieces cut from stock bars: first-fit decreasing.
//!
//! Each bar holds pieces separated by the saw cut (`kerf`): a bar with pieces p₁…pₙ uses
//! Σpᵢ + (n − 1)·kerf ≤ stock. Pieces are placed longest first (stable order, so the plan is
//! deterministic), each in the first bar it fits in. Callers keep rail layers apart: K2 cuts
//! top and bottom rails from separate bars, which reproduces its counts exactly.

use serde::Serialize;

use crate::error::BomError;

/// Bars and the pieces cut from each.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CutPlan {
    pub stock_length_m: f64,
    pub kerf_m: f64,
    /// Pieces of each bar [m], longest first.
    pub bars: Vec<Vec<f64>>,
}

impl CutPlan {
    #[must_use]
    pub fn bar_count(&self) -> u64 {
        self.bars.len() as u64
    }

    /// Length used on a bar, saw cuts included.
    #[must_use]
    pub fn used(&self, bar: &[f64]) -> f64 {
        bar.iter().sum::<f64>() + self.kerf_m * bar.len().saturating_sub(1) as f64
    }
}

/// Tolerance on lengths (pieces are given to the millimetre).
const EPS: f64 = 1e-9;

/// Cuts `pieces` from bars of `stock_m`.
///
/// # Errors
/// [`BomError::InvalidStock`] for a non-positive bar or a negative saw cut,
/// [`BomError::PieceLongerThanStock`] when a piece cannot be cut from one bar.
pub fn cut(
    article_no: &str,
    pieces: &[f64],
    stock_m: f64,
    kerf_m: f64,
) -> Result<CutPlan, BomError> {
    if !(stock_m.is_finite() && stock_m > 0.0 && kerf_m.is_finite() && kerf_m >= 0.0) {
        return Err(BomError::InvalidStock {
            article_no: article_no.into(),
            stock_m,
            kerf_m,
        });
    }
    let mut order: Vec<f64> = pieces.to_vec();
    order.sort_by(|a, b| b.total_cmp(a));
    // Remaining length of each bar, and its pieces.
    let mut free: Vec<f64> = Vec::new();
    let mut bars: Vec<Vec<f64>> = Vec::new();
    for p in order {
        if !(p.is_finite() && p > 0.0 && p <= stock_m + EPS) {
            return Err(BomError::PieceLongerThanStock {
                article_no: article_no.into(),
                length_m: p,
                stock_m,
            });
        }
        match free.iter().position(|&f| f + EPS >= p + kerf_m) {
            Some(i) => {
                free[i] -= p + kerf_m;
                bars[i].push(p);
            }
            None => {
                free.push(stock_m - p);
                bars.push(vec![p]);
            }
        }
    }
    Ok(CutPlan {
        stock_length_m: stock_m,
        kerf_m,
        bars,
    })
}
