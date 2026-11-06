//! Support grids of the roof (standing seams, rafters) and the supports they give a rail.
//!
//! A support grid is a family of parallel lines `x = origin + k·spacing` across the rails
//! (seams of a standing-seam roof, rafters under a tiled or corrugated roof). The position
//! of the grid (its phase) comes from the roof: it decides how many clamps or hanger bolts a
//! given rail gets, which is why the counts can only be exact once the layout knows it.

use serde::{Deserialize, Serialize};

/// Kind of support grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportKind {
    /// One clamp on every seam crossing the rail.
    StandingSeam,
    /// Fixations on rafters / purlins; the statics decide every how many.
    Rafters,
}

/// Lines `origin + k·spacing` (in the coordinate along the rails) where fixations can go.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SupportLines {
    pub kind: SupportKind,
    /// Coordinate of one line [m].
    pub origin_m: f64,
    pub spacing_m: f64,
}

impl SupportLines {
    /// Coordinates of the lines in `[from, to]`.
    #[must_use]
    pub fn between(&self, from: f64, to: f64) -> Vec<f64> {
        grid_positions(from, to, self.origin_m, self.spacing_m)
    }
}

/// Grid lines `origin + k·spacing` in `[lo, hi]`, ascending.
#[must_use]
pub fn grid_positions(lo: f64, hi: f64, origin: f64, spacing: f64) -> Vec<f64> {
    if spacing <= 0.0 || hi < lo {
        return Vec::new();
    }
    let first = ((lo - origin) / spacing - 1e-9).ceil() as i64;
    let last = ((hi - origin) / spacing + 1e-9).floor() as i64;
    (first..=last)
        .map(|k| origin + k as f64 * spacing)
        .collect()
}

/// Number of grid lines under a rail starting at `start_m`, for a grid with a line at
/// `grid_origin_m` and spacing `spacing_m`, keeping at least `min_edge_m` from both rail
/// ends. This is the exact count once the layout gives the grid position.
#[must_use]
pub fn supports_on_grid(
    start_m: f64,
    rail_length_m: f64,
    grid_origin_m: f64,
    spacing_m: f64,
    min_edge_m: f64,
) -> usize {
    grid_positions(
        start_m + min_edge_m,
        start_m + rail_length_m - min_edge_m,
        grid_origin_m,
        spacing_m,
    )
    .len()
}

/// Number of supports under a rail of length `rail_length_m`, centred on a support grid of
/// spacing `spacing_m`, keeping at least `min_overhang_m` at each end.
#[must_use]
pub fn supports_under_centred_rail(
    rail_length_m: f64,
    spacing_m: f64,
    min_overhang_m: f64,
) -> usize {
    if rail_length_m <= 2.0 * min_overhang_m || spacing_m <= 0.0 {
        return 0;
    }
    let usable = rail_length_m - 2.0 * min_overhang_m;
    (usable / spacing_m + 1e-9).floor() as usize + 1
}

/// Climbers (top-rail fixations) of a field: each top rail rests on every bottom rail it
/// crosses; top rails are centred on the bottom rails.
#[must_use]
pub fn climber_count(top_rail_lengths_m: &[f64], bottom_rail_spacing_m: f64) -> usize {
    top_rail_lengths_m
        .iter()
        .map(|&l| supports_under_centred_rail(l, bottom_rail_spacing_m, 0.0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_counting() {
        // 3.06 m rail, 0.6 m seams, first seam 0.03 m after the rail start → 6 clamps.
        assert_eq!(supports_on_grid(0.0, 3.06, 0.03, 0.6, 0.0), 6);
        // Same rail shifted so that the first seam is 0.2 m in → 5 clamps.
        assert_eq!(supports_on_grid(0.0, 3.06, 0.2, 0.6, 0.0), 5);
        assert_eq!(supports_under_centred_rail(2.388, 1.0, 0.0), 3);
        assert_eq!(supports_under_centred_rail(1.234, 1.0, 0.0), 2);
        assert_eq!(supports_under_centred_rail(0.5, 1.0, 0.3), 0);
        assert_eq!(grid_positions(0.0, 1.0, 0.25, 0.5), vec![0.25, 0.75]);
        assert_eq!(grid_positions(-1.0, 0.0, 0.25, 0.5), vec![-0.75, -0.25]);
    }
}
