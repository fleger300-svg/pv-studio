//! Rail pieces cut from stock bars: first-fit decreasing.
//!
//! Each bar holds pieces separated by the saw cut (`kerf`): a bar with pieces p₁…pₙ uses
//! Σpᵢ + (n − 1)·kerf ≤ stock. Pieces are placed longest first (stable order, so the plan is
//! deterministic), each in the first bar it fits in. Callers keep rail layers apart: K2 cuts
//! top and bottom rails from separate bars, which reproduces its counts exactly.
//!
//! A rail longer than a bar is spliced: it takes as many whole bars as it can, and the rest
//! is cut like any other piece (it may share a bar with other pieces). [`joints`] counts the
//! splices (connectors).

use serde::Serialize;

use crate::error::BomError;

/// Bars and the pieces cut from each.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CutPlan {
    pub stock_length_m: f64,
    pub kerf_m: f64,
    /// Pieces of each bar [m], longest first (a long rail appears as its segments).
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
/// A rest shorter than this after whole bars is no piece (it is rounding).
const MIN_REST_M: f64 = 1e-6;

/// Segments of one rail: whole bars, then the rest.
fn segments(length_m: f64, stock_m: f64) -> impl Iterator<Item = f64> {
    let whole = ((length_m + EPS) / stock_m).floor().max(0.0) as usize;
    let rest = length_m - whole as f64 * stock_m;
    std::iter::repeat_n(stock_m, whole).chain((rest > MIN_REST_M).then_some(rest))
}

/// Splices of a rail of `length_m` built from bars of `stock_m` (0 when it fits in one).
#[must_use]
pub fn joints(length_m: f64, stock_m: f64) -> u64 {
    (segments(length_m, stock_m).count() as u64).saturating_sub(1)
}

fn check(article_no: &str, stock_m: f64, kerf_m: f64) -> Result<(), BomError> {
    if stock_m.is_finite() && stock_m > 0.0 && kerf_m.is_finite() && kerf_m >= 0.0 {
        Ok(())
    } else {
        Err(BomError::InvalidStock {
            article_no: article_no.into(),
            stock_m,
            kerf_m,
        })
    }
}

/// Cuts `pieces` from bars of `stock_m`.
///
/// # Errors
/// [`BomError::InvalidStock`] for a non-positive bar or a negative saw cut,
/// [`BomError::InvalidPiece`] for a non-positive or non-finite piece.
pub fn cut(
    article_no: &str,
    pieces: &[f64],
    stock_m: f64,
    kerf_m: f64,
) -> Result<CutPlan, BomError> {
    check(article_no, stock_m, kerf_m)?;
    let mut order: Vec<f64> = Vec::with_capacity(pieces.len());
    for &p in pieces {
        if !(p.is_finite() && p > 0.0) {
            return Err(BomError::InvalidPiece {
                article_no: article_no.into(),
                length_m: p,
            });
        }
        order.extend(segments(p, stock_m));
    }
    order.sort_by(|a, b| b.total_cmp(a));
    // Remaining length of each bar, and its pieces.
    let mut free: Vec<f64> = Vec::new();
    let mut bars: Vec<Vec<f64>> = Vec::new();
    for p in order {
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
