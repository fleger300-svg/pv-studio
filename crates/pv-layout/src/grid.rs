//! Module grid: lattice with mounting blocks, fixed placement and best-offset filling.
//!
//! A lattice places cell `(row, col)` at
//! `origin + (col·pitch_u + ⌊col/block_cols⌋·extra_u, row·pitch_v + ⌊row/block_rows⌋·extra_v)`
//! (the same formula as `pv_model::ArrayGrid::cell_centre`), mirrored for anchors other than
//! the lower-left corner so that partial blocks end up on the side opposite to the anchor.
//! Filling a region tries lattice offsets over one block period and keeps the one that
//! accepts the most modules.

use pv_geo::{OrientedRect, Polygon, Vec2};
use pv_model::{BlockParams, LayoutAnchor};
use serde::Serialize;

use crate::setbacks::BuildableArea;

/// Geometry of one array's grid on the roof plane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GridSpec {
    /// Module footprint on the roof plane along `u` / `v` [m].
    pub footprint_u: f64,
    pub footprint_v: f64,
    /// Clear gap between adjacent modules inside a block [m].
    pub gap_u: f64,
    pub gap_v: f64,
    pub block: Option<BlockParams>,
    /// Row pitch along `v` overriding `footprint_v + gap_v` (tilted rows on flat roofs).
    pub row_pitch_v: Option<f64>,
    pub anchor: LayoutAnchor,
}

impl GridSpec {
    #[must_use]
    pub fn pitch_u(&self) -> f64 {
        self.footprint_u + self.gap_u
    }

    #[must_use]
    pub fn pitch_v(&self) -> f64 {
        self.row_pitch_v.unwrap_or(self.footprint_v + self.gap_v)
    }

    /// (block_cols, block_rows, extra_u, extra_v); zeros without blocks.
    #[must_use]
    pub fn block_terms(&self) -> (u32, u32, f64, f64) {
        match self.block {
            Some(b) => (
                b.max_cols,
                b.max_rows,
                b.gap_u_m - self.gap_u,
                b.gap_v_m - self.gap_v,
            ),
            None => (0, 0, 0.0, 0.0),
        }
    }

    /// Distance from the centre of column 0 to the centre of column `col` [m].
    #[must_use]
    pub fn col_offset(&self, col: u32) -> f64 {
        let (bc, _, eu, _) = self.block_terms();
        let blocks = col.checked_div(bc).unwrap_or(0);
        f64::from(col) * self.pitch_u() + f64::from(blocks) * eu
    }

    #[must_use]
    pub fn row_offset(&self, row: u32) -> f64 {
        let (_, br, _, ev) = self.block_terms();
        let blocks = row.checked_div(br).unwrap_or(0);
        f64::from(row) * self.pitch_v() + f64::from(blocks) * ev
    }

    /// Overall size of a `cols × rows` field (module edges) [m].
    #[must_use]
    pub fn extent(&self, cols: u32, rows: u32) -> (f64, f64) {
        if cols == 0 || rows == 0 {
            return (0.0, 0.0);
        }
        (
            self.col_offset(cols - 1) + self.footprint_u,
            self.row_offset(rows - 1) + self.footprint_v,
        )
    }

    /// Period of the lattice (one block, or one cell without blocks) along `u` / `v`.
    #[must_use]
    pub fn period(&self) -> (f64, f64) {
        let (bc, br, _, _) = self.block_terms();
        let pu = if bc > 0 {
            self.col_offset(bc)
        } else {
            self.pitch_u()
        };
        let pv = if br > 0 {
            self.row_offset(br)
        } else {
            self.pitch_v()
        };
        (pu, pv)
    }

    fn signs(&self) -> (f64, f64) {
        match self.anchor {
            LayoutAnchor::LowerLeft | LayoutAnchor::Centre => (1.0, 1.0),
            LayoutAnchor::LowerRight => (-1.0, 1.0),
            LayoutAnchor::UpperLeft => (1.0, -1.0),
            LayoutAnchor::UpperRight => (-1.0, -1.0),
        }
    }
}

/// One placed cell.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Cell {
    pub row: u32,
    pub col: u32,
    /// Centre of the module footprint.
    pub centre: Vec2,
}

/// Result of a placement: the lattice actually used and the accepted cells. After a fill,
/// indices are shifted by whole blocks so that the first occupied block is block 0 (block
/// membership is preserved; the first occupied column can be 1 inside that block).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placement {
    pub spec: GridSpec,
    /// Centre of cell (0, 0) in roof coordinates.
    pub origin: Vec2,
    pub rows: u32,
    pub cols: u32,
    pub cells: Vec<Cell>,
}

impl Placement {
    #[must_use]
    pub fn count(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn footprint(&self, cell: &Cell) -> OrientedRect {
        OrientedRect::axis_aligned(cell.centre, self.spec.footprint_u, self.spec.footprint_v)
    }
}

/// Region a field may use: the buildable area of the face, optionally restricted to a field
/// outline drawn by the user.
#[derive(Debug, Clone, Copy)]
pub struct Region<'a> {
    pub area: &'a BuildableArea,
    pub boundary: Option<&'a Polygon>,
}

impl Region<'_> {
    #[must_use]
    pub fn accepts(&self, r: &OrientedRect) -> bool {
        self.boundary.is_none_or(|b| b.contains_rect(r)) && self.area.accepts(r)
    }

    /// Bounding box of the usable region.
    #[must_use]
    pub fn bounds(&self) -> (Vec2, Vec2) {
        let (lo, hi) = self.area.roof().boundary().bounds();
        match self.boundary {
            Some(b) => {
                let (blo, bhi) = b.bounds();
                (
                    Vec2::new(lo.u.max(blo.u), lo.v.max(blo.v)),
                    Vec2::new(hi.u.min(bhi.u), hi.v.min(bhi.v)),
                )
            }
            None => (lo, hi),
        }
    }
}

fn centre_of(spec: &GridSpec, origin: Vec2, row: u32, col: u32) -> Vec2 {
    let (su, sv) = spec.signs();
    Vec2::new(
        origin.u + su * spec.col_offset(col),
        origin.v + sv * spec.row_offset(row),
    )
}

/// Places a `rows × cols` field with cell (0, 0) centred at `origin`, keeping the cells the
/// region accepts.
#[must_use]
pub fn place_fixed(
    region: Region<'_>,
    spec: GridSpec,
    origin: Vec2,
    rows: u32,
    cols: u32,
) -> Placement {
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let centre = centre_of(&spec, origin, row, col);
            if region.accepts(&OrientedRect::axis_aligned(
                centre,
                spec.footprint_u,
                spec.footprint_v,
            )) {
                cells.push(Cell { row, col, centre });
            }
        }
    }
    Placement {
        spec,
        origin,
        rows,
        cols,
        cells,
    }
}

/// Offset search resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Search {
    /// Coarse step over one lattice period [m].
    pub coarse_m: f64,
    /// Fine step around the best coarse candidates [m].
    pub fine_m: f64,
    /// How many coarse candidates are refined.
    pub refine_best: usize,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            coarse_m: 0.05,
            fine_m: 0.005,
            refine_best: 3,
        }
    }
}

/// Fills the region with as many modules as possible: tries lattice offsets over one block
/// period (coarse, then fine around the best), plus the offsets that put a module edge on
/// the region's anchor-side bounds. Ties go to the layout closest to the anchor (or to the
/// region centre for [`LayoutAnchor::Centre`]). Deterministic.
#[must_use]
pub fn fill(region: Region<'_>, spec: GridSpec, search: Search) -> Placement {
    let (lo, hi) = region.bounds();
    let (su, sv) = spec.signs();
    // Lattice start: centre of cell (0, 0) when the offset is zero, flush with the anchor side.
    let start = Vec2::new(
        if su > 0.0 {
            lo.u + spec.footprint_u / 2.0
        } else {
            hi.u - spec.footprint_u / 2.0
        },
        if sv > 0.0 {
            lo.v + spec.footprint_v / 2.0
        } else {
            hi.v - spec.footprint_v / 2.0
        },
    );
    let (pu, pv) = spec.period();
    let span = hi - lo;
    // Enough cells to cover the region whatever the offset.
    let cols = (span.u / spec.pitch_u()).ceil() as u32 + 1;
    let rows = (span.v / spec.pitch_v()).ceil() as u32 + 1;

    let (hu, hv) = (spec.footprint_u / 2.0, spec.footprint_v / 2.0);
    let centre_target = (lo + hi) * 0.5;
    // Score of one offset without building the placement: (count, tie key). Cells whose
    // footprint leaves the region's bounding box are rejected before the exact test.
    let score = |du: f64, dv: f64| -> (usize, f64) {
        let origin = Vec2::new(start.u + su * du, start.v + sv * dv);
        let (mut n, mut sum) = (0usize, Vec2::ZERO);
        for row in 0..rows {
            let cv = origin.v + sv * spec.row_offset(row);
            if cv - hv < lo.v - 1e-9 || cv + hv > hi.v + 1e-9 {
                continue;
            }
            for col in 0..cols {
                let cu = origin.u + su * spec.col_offset(col);
                if cu - hu < lo.u - 1e-9 || cu + hu > hi.u + 1e-9 {
                    continue;
                }
                let centre = Vec2::new(cu, cv);
                if region.accepts(&OrientedRect::axis_aligned(
                    centre,
                    spec.footprint_u,
                    spec.footprint_v,
                )) {
                    n += 1;
                    sum = sum + centre;
                }
            }
        }
        let key = if spec.anchor == LayoutAnchor::Centre && n > 0 {
            (sum * (1.0 / n as f64)).distance(centre_target)
        } else {
            dv * 1000.0 + du
        };
        (n, key)
    };
    let better = |a: (usize, f64), b: (usize, f64)| a.0 > b.0 || (a.0 == b.0 && a.1 < b.1 - 1e-12);

    let steps = |period: f64, step: f64| -> Vec<f64> {
        let n = (period / step).ceil().max(1.0) as usize;
        (0..n)
            .map(|i| i as f64 * step)
            .filter(|&x| x < period)
            .collect()
    };
    let mut scored: Vec<((usize, f64), f64, f64)> = Vec::new();
    for dv in steps(pv, search.coarse_m) {
        for du in steps(pu, search.coarse_m) {
            scored.push((score(du, dv), du, dv));
        }
    }
    scored.sort_by(|a, b| b.0 .0.cmp(&a.0 .0).then(a.0 .1.total_cmp(&b.0 .1)));
    scored.truncate(search.refine_best.max(1));

    let mut best = scored[0];
    let n = (search.coarse_m / search.fine_m).round() as i32;
    for &(_, du0, dv0) in &scored {
        for j in -n..=n {
            for i in -n..=n {
                let (du, dv) = (
                    du0 + f64::from(i) * search.fine_m,
                    dv0 + f64::from(j) * search.fine_m,
                );
                if du < 0.0 || dv < 0.0 {
                    continue;
                }
                let sc = score(du, dv);
                if better(sc, best.0) {
                    best = (sc, du, dv);
                }
            }
        }
    }
    let (_, du, dv) = best;
    let mut p = place_fixed(
        region,
        spec,
        Vec2::new(start.u + su * du, start.v + sv * dv),
        rows,
        cols,
    );
    reindex(&mut p);
    p
}

/// Shifts row / column indices so that the first occupied row and column are 0, keeping
/// block membership: the shift is a whole number of blocks, the origin moves accordingly.
fn reindex(p: &mut Placement) {
    if p.cells.is_empty() {
        p.rows = 0;
        p.cols = 0;
        return;
    }
    let (bc, br, _, _) = p.spec.block_terms();
    let min_col = p.cells.iter().map(|c| c.col).min().unwrap_or(0);
    let min_row = p.cells.iter().map(|c| c.row).min().unwrap_or(0);
    let shift_col = min_col.checked_div(bc).map_or(min_col, |b| b * bc);
    let shift_row = min_row.checked_div(br).map_or(min_row, |b| b * br);
    let (su, sv) = p.spec.signs();
    p.origin = Vec2::new(
        p.origin.u + su * p.spec.col_offset(shift_col),
        p.origin.v + sv * p.spec.row_offset(shift_row),
    );
    for c in &mut p.cells {
        c.col -= shift_col;
        c.row -= shift_row;
    }
    p.cols = p.cells.iter().map(|c| c.col).max().unwrap_or(0) + 1;
    p.rows = p.cells.iter().map(|c| c.row).max().unwrap_or(0) + 1;
}

// ---------------------------------------------------------------------------------------------
// East-west pairs
// ---------------------------------------------------------------------------------------------

/// East-west system: pairs of modules leaning on each other along `u`, rows of pairs
/// along `v`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EastWestSpec {
    /// Footprint of one tilted module along `u` (projected depth) and `v`.
    pub footprint_u: f64,
    pub footprint_v: f64,
    /// Gap between the two modules of a pair at the peak [m].
    pub peak_gap: f64,
    /// Pitch of the pairs along `u` (pair + service corridor) [m].
    pub pair_pitch_u: f64,
    /// Gap between modules along `v` [m].
    pub gap_v: f64,
    pub block: Option<BlockParams>,
    pub anchor: LayoutAnchor,
}

/// Placement of an east-west field: the two modules of every accepted pair.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EastWestPlacement {
    /// Pairs (one cell per pair, centred on the pair).
    pub pairs: Placement,
    /// Modules on the −u side of each pair (azimuth offset +90°).
    pub minus_u: Vec<Cell>,
    /// Modules on the +u side of each pair (azimuth offset −90°).
    pub plus_u: Vec<Cell>,
}

#[must_use]
pub fn fill_east_west(region: Region<'_>, spec: EastWestSpec, search: Search) -> EastWestPlacement {
    let pair_u = 2.0 * spec.footprint_u + spec.peak_gap;
    let grid = GridSpec {
        footprint_u: pair_u,
        footprint_v: spec.footprint_v,
        gap_u: spec.pair_pitch_u - pair_u,
        gap_v: spec.gap_v,
        block: spec.block,
        row_pitch_v: None,
        anchor: spec.anchor,
    };
    let pairs = fill(region, grid, search);
    let half = (spec.footprint_u + spec.peak_gap) / 2.0;
    let module = |c: &Cell, sign: f64| Cell {
        centre: Vec2::new(c.centre.u + sign * half, c.centre.v),
        ..*c
    };
    let minus_u = pairs.cells.iter().map(|c| module(c, -1.0)).collect();
    let plus_u = pairs.cells.iter().map(|c| module(c, 1.0)).collect();
    EastWestPlacement {
        pairs,
        minus_u,
        plus_u,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RoofOutline, SetbackRules};

    fn spec() -> GridSpec {
        GridSpec {
            footprint_u: 1.8,
            footprint_v: 1.134,
            gap_u: 0.01,
            gap_v: 0.02,
            block: Some(BlockParams {
                max_cols: 2,
                max_rows: 2,
                gap_u_m: 0.08,
                gap_v_m: 0.13,
            }),
            row_pitch_v: None,
            anchor: LayoutAnchor::LowerLeft,
        }
    }

    #[test]
    fn lattice_offsets_with_blocks() {
        let s = spec();
        assert!((s.col_offset(1) - 1.81).abs() < 1e-12);
        assert!((s.col_offset(2) - (2.0 * 1.81 + 0.07)).abs() < 1e-12);
        let (w, h) = s.extent(5, 4);
        assert!((w - 9.18).abs() < 1e-9, "{w}");
        assert!((h - 4.706).abs() < 1e-9, "{h}");
    }

    #[test]
    fn fill_matches_fixed_on_exact_rectangle() {
        let roof = RoofOutline::rectangle(7.30, 4.71, crate::EdgeKind::Verge).unwrap();
        let area = BuildableArea::new(roof, &SetbackRules::uniform(0.0, 0.0));
        let p = fill(
            Region {
                area: &area,
                boundary: None,
            },
            spec(),
            Search::default(),
        );
        assert_eq!(p.count(), 16);
        assert_eq!((p.rows, p.cols), (4, 4));
    }

    #[test]
    fn anchor_moves_partial_blocks() {
        // Room for 3 columns: with a lower-left anchor the single column is on the right.
        let roof = RoofOutline::rectangle(5.49, 1.134, crate::EdgeKind::Verge).unwrap();
        let area = BuildableArea::new(roof, &SetbackRules::uniform(0.0, 0.0));
        let region = Region {
            area: &area,
            boundary: None,
        };
        let ll = fill(region, spec(), Search::default());
        let lr = fill(
            region,
            GridSpec {
                anchor: LayoutAnchor::LowerRight,
                ..spec()
            },
            Search::default(),
        );
        assert_eq!((ll.count(), lr.count()), (3, 3));
        let lone = |p: &Placement| p.cells.iter().find(|c| c.col == 2).unwrap().centre.u;
        assert!(lone(&ll) > 4.0, "lower-left: lone column at the right");
        assert!(lone(&lr) < 1.5, "lower-right: lone column at the left");
    }

    #[test]
    fn east_west_pairs_are_split() {
        let roof = RoofOutline::flat(10.0, 6.0).unwrap();
        let area = BuildableArea::new(roof, &SetbackRules::uniform(0.5, 0.0));
        let ew = EastWestSpec {
            footprint_u: 1.121,
            footprint_v: 1.762,
            peak_gap: 0.08,
            pair_pitch_u: 2.46,
            gap_v: 0.038,
            block: None,
            anchor: LayoutAnchor::LowerLeft,
        };
        let p = fill_east_west(
            Region {
                area: &area,
                boundary: None,
            },
            ew,
            Search::default(),
        );
        // 9 m usable / 2.46 → 3 pairs (3·2.46 − 0.138 = 7.24 m), 5 m / 1.8 → 2 rows.
        assert_eq!(p.pairs.count(), 6);
        assert_eq!(p.minus_u.len(), 6);
        for (w, e) in p.minus_u.iter().zip(&p.plus_u) {
            assert!(w.centre.u < e.centre.u);
            assert!((e.centre.u - w.centre.u - (1.121 + 0.08)).abs() < 1e-12);
        }
    }
}
