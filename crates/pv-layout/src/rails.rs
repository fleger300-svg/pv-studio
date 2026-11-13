//! Rail tracing for the two-layer cross-rail system (K2 SingleRail with climbers).
//!
//! - Top rails run along `v` (up the slope): `per_column` rails per module column of a block,
//!   one rail per run of vertically adjacent modules, overhanging the module edges by
//!   `top_overhang_m`. Modules are clamped on them.
//! - Each top rail rests on bottom rails at support lines along `v`. The reference column
//!   (the block's full height) gets the largest number of supports at `bottom_spacing_m`
//!   that keeps `min_support_overhang_m` at both ends, centred. A shorter column starting at
//!   the block's bottom row reuses the reference first support and, if its last support
//!   does not fit, reduces the spacing in `spacing_step_m` steps; one starting higher is
//!   centred the same way on its own rail.
//! - Bottom rails run along `u`: one per support line, continuous over adjacent columns that
//!   share the line, ending `bottom_inset_m` inside the outer module edges.
//! - Climbers are at the crossings; fixations at the roof's support lines under the bottom
//!   rails ([`fixations`]).
//!
//! The defaults reproduce K2 Base 3.2.109 on project A (top rails 2.388 / 1.234 m, bottom
//! rails 3.06 / 1.25 m base length, reduced spacing 0.98 m, rail and climber counts).

use std::collections::BTreeMap;

use pv_geo::Vec2;
use pv_model::RailLayers;
use serde::{Deserialize, Serialize};

use crate::blocks::Structure;
use crate::grid::{Cell, GridSpec};
use crate::supports::{grid_positions, supports_under_centred_rail, SupportLines};

/// Geometry rules of the cross-rail system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrossRailParams {
    /// Two layers (top rails on bottom rails) or one (rails along `u` carrying the modules,
    /// see [`pv_model::RailLayers::One`] for how the other fields are read then).
    #[serde(default)]
    pub layers: RailLayers,
    /// Top rails per module column.
    pub per_column: u32,
    /// Distance from the module edge (along `u`) to the outer top rails [m].
    pub top_inset_m: f64,
    /// Top-rail overhang beyond the module edges along `v` [m].
    pub top_overhang_m: f64,
    /// Nominal spacing of the bottom rails (K2: « Distance des Rails ») [m].
    pub bottom_spacing_m: f64,
    /// Minimum distance from a top-rail end to its outer support [m].
    pub min_support_overhang_m: f64,
    /// Step by which the spacing is reduced when the last support does not fit [m].
    pub spacing_step_m: f64,
    /// Bottom-rail end inside the outer module edge along `u` [m].
    pub bottom_inset_m: f64,
}

impl CrossRailParams {
    /// K2 SingleRail cross system as calibrated on project A. `top_inset_m` is not printed
    /// by K2 (it only matters for climber positions along the bottom rails); 0.40 m is a
    /// typical clamping position for a 1.8 m module.
    #[must_use]
    pub fn k2_single_rail() -> Self {
        Self {
            layers: RailLayers::Two,
            per_column: 2,
            top_inset_m: 0.40,
            top_overhang_m: 0.05,
            bottom_spacing_m: 1.00,
            min_support_overhang_m: 0.06,
            spacing_step_m: 0.01,
            bottom_inset_m: 0.275,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RailLayer {
    Top,
    Bottom,
}

/// A straight rail.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rail {
    pub block: usize,
    pub layer: RailLayer,
    pub start: Vec2,
    pub end: Vec2,
}

impl Rail {
    #[must_use]
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }
}

/// Supports of one top rail on the bottom rails.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopSupports {
    /// Index into [`RailLayout::top`].
    pub top_rail: usize,
    /// `v` of each support, ascending.
    pub v: Vec<f64>,
    /// Spacing actually used (reduced for short columns) [m].
    pub spacing_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RailLayout {
    pub top: Vec<Rail>,
    /// Modules carried by each top rail (aligned with `top`): the run of adjacent modules of
    /// its column. Middle clamps = modules − 1, end clamps = 2 per rail.
    pub top_modules: Vec<u32>,
    pub bottom: Vec<Rail>,
    /// Modules carried by each bottom rail (aligned with `bottom`) in a one-layer system,
    /// where the bottom rails are the module-carrying rails; empty in a two-layer system.
    #[serde(default)]
    pub bottom_modules: Vec<u32>,
    pub supports: Vec<TopSupports>,
    /// Crossings of top and bottom rails.
    pub climbers: Vec<Vec2>,
}

fn key(x: f64) -> i64 {
    (x * 1e6).round() as i64
}

/// Traces the rails of every mounting block of a field.
#[must_use]
pub fn cross_rails(
    cells: &[Cell],
    spec: &GridSpec,
    structure: &Structure,
    p: &CrossRailParams,
) -> RailLayout {
    let (hu, hv) = (spec.footprint_u / 2.0, spec.footprint_v / 2.0);
    let mut out = RailLayout {
        top: Vec::new(),
        top_modules: Vec::new(),
        bottom: Vec::new(),
        bottom_modules: Vec::new(),
        supports: Vec::new(),
        climbers: Vec::new(),
    };
    if p.layers == RailLayers::One {
        one_layer(cells, hu, hv, structure, p, &mut out);
        return out;
    }

    for block in &structure.blocks {
        // Columns of the block: col → (u centre, sorted (row, v centre)).
        let mut columns: BTreeMap<u32, (f64, Vec<(u32, f64)>)> = BTreeMap::new();
        for &i in &block.cells {
            let c = &cells[i];
            let e = columns.entry(c.col).or_insert((c.centre.u, Vec::new()));
            e.1.push((c.row, c.centre.v));
        }
        let block_bottom_row = block.cells.iter().map(|&i| cells[i].row).min().unwrap_or(0);
        let rail_span =
            |lo_v: f64, hi_v: f64| (lo_v - hv - p.top_overhang_m, hi_v + hv + p.top_overhang_m);

        // Reference column: the block's full height.
        let (ref_lo, ref_hi) = rail_span(
            block
                .cells
                .iter()
                .map(|&i| cells[i].centre.v)
                .fold(f64::INFINITY, f64::min),
            block
                .cells
                .iter()
                .map(|&i| cells[i].centre.v)
                .fold(f64::NEG_INFINITY, f64::max),
        );
        let ref_n = supports_under_centred_rail(
            ref_hi - ref_lo,
            p.bottom_spacing_m,
            p.min_support_overhang_m,
        )
        .max(1);
        let ref_first_overhang = (ref_hi - ref_lo - (ref_n - 1) as f64 * p.bottom_spacing_m) / 2.0;
        let ref_first = ref_lo + ref_first_overhang;

        // Support lines per column: v key → columns using it.
        let mut lines: BTreeMap<i64, (f64, Vec<u32>)> = BTreeMap::new();
        for (&col, (u, rows)) in &mut columns {
            rows.sort_by_key(|r| r.0);
            // Runs of vertically adjacent modules.
            let mut runs: Vec<Vec<(u32, f64)>> = Vec::new();
            for &r in rows.iter() {
                match runs.last_mut() {
                    Some(run) if run.last().is_some_and(|l| l.0 + 1 == r.0) => run.push(r),
                    _ => runs.push(vec![r]),
                }
            }
            for run in runs {
                let (lo, hi) = rail_span(run[0].1, run[run.len() - 1].1);
                let n = supports_under_centred_rail(
                    hi - lo,
                    p.bottom_spacing_m,
                    p.min_support_overhang_m,
                )
                .max(1);
                let first = if run[0].0 == block_bottom_row {
                    ref_first
                } else {
                    lo + ref_first_overhang
                };
                let mut spacing = p.bottom_spacing_m;
                if n > 1 && first + (n - 1) as f64 * spacing > hi - p.min_support_overhang_m + 1e-9
                {
                    let room = (hi - p.min_support_overhang_m - first) / (n - 1) as f64;
                    spacing = ((room / p.spacing_step_m) + 1e-9).floor() * p.spacing_step_m;
                }
                let vs: Vec<f64> = (0..n).map(|k| first + k as f64 * spacing).collect();
                // Top rails of this run.
                let k = p.per_column.max(1);
                for r in 0..k {
                    let tu = if k == 1 {
                        *u
                    } else {
                        let left = *u - hu + p.top_inset_m;
                        let right = *u + hu - p.top_inset_m;
                        left + (right - left) * f64::from(r) / f64::from(k - 1)
                    };
                    let idx = out.top.len();
                    out.top.push(Rail {
                        block: block.id,
                        layer: RailLayer::Top,
                        start: Vec2::new(tu, lo),
                        end: Vec2::new(tu, hi),
                    });
                    out.top_modules
                        .push(u32::try_from(run.len()).unwrap_or(u32::MAX));
                    out.climbers.extend(vs.iter().map(|&v| Vec2::new(tu, v)));
                    out.supports.push(TopSupports {
                        top_rail: idx,
                        v: vs.clone(),
                        spacing_m: spacing,
                    });
                }
                for &v in &vs {
                    let e = lines.entry(key(v)).or_insert((v, Vec::new()));
                    if !e.1.contains(&col) {
                        e.1.push(col);
                    }
                }
            }
        }

        // Bottom rails: one per support line and run of adjacent columns sharing it.
        for (v, mut cols) in lines.into_values() {
            cols.sort_unstable();
            let mut runs: Vec<Vec<u32>> = Vec::new();
            for c in cols {
                match runs.last_mut() {
                    Some(run) if run.last().is_some_and(|&l| l + 1 == c) => run.push(c),
                    _ => runs.push(vec![c]),
                }
            }
            for run in runs {
                let us: Vec<f64> = run.iter().map(|c| columns[c].0).collect();
                let lo = us.iter().copied().fold(f64::INFINITY, f64::min) - hu + p.bottom_inset_m;
                let hi =
                    us.iter().copied().fold(f64::NEG_INFINITY, f64::max) + hu - p.bottom_inset_m;
                out.bottom.push(Rail {
                    block: block.id,
                    layer: RailLayer::Bottom,
                    start: Vec2::new(lo, v),
                    end: Vec2::new(hi, v),
                });
            }
        }
    }
    out
}

/// One-layer rails: per run of horizontally adjacent modules of a block row, `per_column`
/// rails along `u`, `top_inset_m` inside the module edges along `v`, overhanging the
/// outer modules by `top_overhang_m`. They are the rails fixed to the roof (`bottom`).
fn one_layer(
    cells: &[Cell],
    hu: f64,
    hv: f64,
    structure: &Structure,
    p: &CrossRailParams,
    out: &mut RailLayout,
) {
    for block in &structure.blocks {
        // Rows of the block: row → (v centre, sorted (col, u centre)).
        let mut rows: BTreeMap<u32, (f64, Vec<(u32, f64)>)> = BTreeMap::new();
        for &i in &block.cells {
            let c = &cells[i];
            let e = rows.entry(c.row).or_insert((c.centre.v, Vec::new()));
            e.1.push((c.col, c.centre.u));
        }
        for (v, cols) in rows.values_mut() {
            cols.sort_by_key(|c| c.0);
            let mut runs: Vec<Vec<(u32, f64)>> = Vec::new();
            for &c in cols.iter() {
                match runs.last_mut() {
                    Some(run) if run.last().is_some_and(|l| l.0 + 1 == c.0) => run.push(c),
                    _ => runs.push(vec![c]),
                }
            }
            let k = p.per_column.max(1);
            for run in runs {
                let lo = run[0].1 - hu - p.top_overhang_m;
                let hi = run[run.len() - 1].1 + hu + p.top_overhang_m;
                for r in 0..k {
                    let rv = if k == 1 {
                        *v
                    } else {
                        let low = *v - hv + p.top_inset_m;
                        let high = *v + hv - p.top_inset_m;
                        low + (high - low) * f64::from(r) / f64::from(k - 1)
                    };
                    out.bottom.push(Rail {
                        block: block.id,
                        layer: RailLayer::Bottom,
                        start: Vec2::new(lo, rv),
                        end: Vec2::new(hi, rv),
                    });
                    out.bottom_modules
                        .push(u32::try_from(run.len()).unwrap_or(u32::MAX));
                }
            }
        }
    }
}

/// Keeps every `every`-th fixation position (ascending), always keeping the first and the
/// last one: gaps never exceed `every` grid spacings and the rail-end cantilevers do not
/// change.
#[must_use]
pub fn every_nth(positions: Vec<f64>, every: u32) -> Vec<f64> {
    let n = every.max(1) as usize;
    if n == 1 || positions.len() <= 2 {
        return positions;
    }
    let last = positions.len() - 1;
    positions
        .iter()
        .enumerate()
        .filter(|(i, _)| i % n == 0 || *i == last)
        .map(|(_, &x)| x)
        .collect()
}

/// Fixations of the bottom rails on the roof's support lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fixations {
    /// Bottom rails, possibly lengthened to reach an extra support line.
    pub bottom: Vec<Rail>,
    /// `u` of the fixations of each bottom rail.
    pub positions: Vec<Vec<f64>>,
}

impl Fixations {
    #[must_use]
    pub fn count(&self) -> usize {
        self.positions.iter().map(Vec::len).sum()
    }
}

/// Places the fixations of each bottom rail on every support line under it, keeping
/// `min_edge_m` from the rail ends. With `max_cantilever_m` (K2: « longueur maximale en
/// porte-à-faux » of the bottom rail), a rail end further than that from its outer
/// fixation is lengthened to the next support line + `min_edge_m`, adding one fixation.
/// This bounds the lengthening to `spacing − max_cantilever + min_edge` (0.145 m for K2's
/// 0.485 m on 0.6 m seams), which brackets every lengthened rail of project A.
///
/// `every`: fixations on every n-th support line only (rafters: the statics decide n), see
/// [`every_nth`]; 1 on standing seams.
#[must_use]
pub fn fixations(
    rails: &RailLayout,
    lines: &SupportLines,
    min_edge_m: f64,
    max_cantilever_m: Option<f64>,
    every: u32,
) -> Fixations {
    let mut bottom = rails.bottom.clone();
    let mut positions = Vec::with_capacity(bottom.len());
    for rail in &mut bottom {
        let v = rail.start.v;
        let (mut lo, mut hi) = (rail.start.u.min(rail.end.u), rail.start.u.max(rail.end.u));
        if let Some(max) = max_cantilever_m {
            let fx = grid_positions(
                lo + min_edge_m,
                hi - min_edge_m,
                lines.origin_m,
                lines.spacing_m,
            );
            let first = fx.first().copied();
            let last = fx.last().copied();
            if first.is_none_or(|f| f - lo > max + 1e-9) {
                if let Some(&s) = grid_positions(
                    lo - lines.spacing_m,
                    lo + min_edge_m - 1e-9,
                    lines.origin_m,
                    lines.spacing_m,
                )
                .last()
                {
                    lo = s - min_edge_m;
                }
            }
            if last.is_none_or(|l| hi - l > max + 1e-9) {
                if let Some(&s) = grid_positions(
                    hi - min_edge_m + 1e-9,
                    hi + lines.spacing_m,
                    lines.origin_m,
                    lines.spacing_m,
                )
                .first()
                {
                    hi = s + min_edge_m;
                }
            }
        }
        rail.start = Vec2::new(lo, v);
        rail.end = Vec2::new(hi, v);
        positions.push(every_nth(
            grid_positions(
                lo + min_edge_m,
                hi - min_edge_m,
                lines.origin_m,
                lines.spacing_m,
            ),
            every,
        ));
    }
    Fixations { bottom, positions }
}
