//! Mounting blocks and mechanical links.
//!
//! A mounting block is a set of modules carried by the same continuous rails. It is built in
//! three steps:
//!
//! 1. the lattice block of each module (`GridSpec::block`, e.g. K2's 2 × 2 blocks); without
//!    blocks the whole field is one lattice block;
//! 2. expansion joints cut each lattice block along `u` and `v` ([`crate::joints`]);
//! 3. the modules left in one part are grouped by 4-adjacency (a module touching another
//!    along a side). A missing module therefore splits a row only when nothing else holds the
//!    two sides together; this never links modules that are not physically adjacent, which is
//!    on the safe side for ballast sharing.
//!
//! Links are the 4-adjacent pairs inside a lattice block; a link crossing an expansion joint
//! is kept but flagged, so that `pv-statics` can drop it from ballast sharing (as the
//! architecture requires: « les joints de dilatation coupent ces liaisons »). Modules of two
//! different lattice blocks are not linked at all.

use std::collections::BTreeMap;

use pv_geo::Vec2;
use pv_model::JointAxis;
use serde::Serialize;

use crate::grid::{Cell, GridSpec};
use crate::joints::{split_line, JointRules, JointSource};
use crate::LayoutError;

/// One mounting block.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MountingBlock {
    pub id: usize,
    /// Indices into the cells given to [`build_structure`].
    pub cells: Vec<usize>,
    /// Lattice block it belongs to `(block col, block row)`.
    pub lattice_block: (u32, u32),
    /// Number of distinct columns / rows it spans.
    pub cols: u32,
    pub rows: u32,
    /// Module-edge bounds on the roof.
    pub lo: Vec2,
    pub hi: Vec2,
}

impl MountingBlock {
    /// Length along `u` and `v` (module edge to module edge) [m].
    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        (self.hi.u - self.lo.u, self.hi.v - self.lo.v)
    }
}

/// Mechanical link between two adjacent modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CellLink {
    pub a: usize,
    pub b: usize,
    pub crosses_expansion_joint: bool,
}

/// An expansion joint actually placed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Joint {
    pub axis: JointAxis,
    /// Coordinate of the joint line, in the middle of the module gap [m].
    pub position_m: f64,
    /// Extent of the line along the other axis (the lattice block's modules) [m].
    pub from_m: f64,
    pub to_m: f64,
    pub source: JointSource,
    pub lattice_block: (u32, u32),
}

/// Blocks, links and joints of one field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Structure {
    pub blocks: Vec<MountingBlock>,
    /// Block id of every cell.
    pub block_of: Vec<usize>,
    pub links: Vec<CellLink>,
    pub joints: Vec<Joint>,
}

impl Structure {
    /// Links that transmit forces (not across an expansion joint).
    pub fn rigid_links(&self) -> impl Iterator<Item = &CellLink> {
        self.links.iter().filter(|l| !l.crosses_expansion_joint)
    }
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    let mut j = i;
    while parent[j] != r {
        let next = parent[j];
        parent[j] = r;
        j = next;
    }
    r
}

/// Builds the mounting blocks of a field from its cells (active modules only).
pub fn build_structure(
    cells: &[Cell],
    spec: &GridSpec,
    rules: &JointRules,
) -> Result<Structure, LayoutError> {
    let (bc, br, _, _) = spec.block_terms();
    let lattice = |c: &Cell| {
        (
            c.col.checked_div(bc).unwrap_or(0),
            c.row.checked_div(br).unwrap_or(0),
        )
    };
    let (hu, hv) = (spec.footprint_u / 2.0, spec.footprint_v / 2.0);

    // Per lattice block: distinct column / row centres, split by the joint rules.
    let mut members: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, c) in cells.iter().enumerate() {
        members.entry(lattice(c)).or_default().push(i);
    }
    let mut part_u: BTreeMap<((u32, u32), u32), usize> = BTreeMap::new(); // (lattice, col) → part
    let mut part_v: BTreeMap<((u32, u32), u32), usize> = BTreeMap::new(); // (lattice, row) → part
    let mut joints = Vec::new();
    let manual_u: Vec<f64> = rules.manual_positions(JointAxis::ConstantU).collect();
    let manual_v: Vec<f64> = rules.manual_positions(JointAxis::ConstantV).collect();
    for (&lb, idx) in &members {
        let span = |f: &dyn Fn(&Cell) -> f64| {
            idx.iter()
                .map(|&i| f(&cells[i]))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
                    (a.min(x), b.max(x))
                })
        };
        let (v_lo, v_hi) = span(&|c| c.centre.v);
        let (u_lo, u_hi) = span(&|c| c.centre.u);
        for (axis, max, manual, footprint, store) in [
            (
                JointAxis::ConstantU,
                rules.max_length_u,
                &manual_u,
                spec.footprint_u,
                &mut part_u,
            ),
            (
                JointAxis::ConstantV,
                rules.max_length_v,
                &manual_v,
                spec.footprint_v,
                &mut part_v,
            ),
        ] {
            // (index, centre) of each distinct column / row, sorted by coordinate.
            let mut lines: Vec<(u32, f64)> = idx
                .iter()
                .map(|&i| match axis {
                    JointAxis::ConstantU => (cells[i].col, cells[i].centre.u),
                    JointAxis::ConstantV => (cells[i].row, cells[i].centre.v),
                })
                .collect();
            lines.sort_by(|a, b| a.1.total_cmp(&b.1));
            lines.dedup_by_key(|l| l.0);
            let centres: Vec<f64> = lines.iter().map(|l| l.1).collect();
            let (parts, cuts) = split_line(&centres, footprint, max, manual).ok_or(
                LayoutError::JointRuleUnsatisfiable {
                    axis,
                    module_m: footprint,
                    max_m: max.unwrap_or(0.0),
                },
            )?;
            for (k, &(line, _)) in lines.iter().enumerate() {
                store.insert((lb, line), parts[k]);
            }
            for (i, source) in cuts {
                let position_m = (centres[i] + centres[i + 1]) / 2.0;
                let (from_m, to_m) = match axis {
                    JointAxis::ConstantU => (v_lo - hv, v_hi + hv),
                    JointAxis::ConstantV => (u_lo - hu, u_hi + hu),
                };
                joints.push(Joint {
                    axis,
                    position_m,
                    from_m,
                    to_m,
                    source,
                    lattice_block: lb,
                });
            }
        }
    }
    let unit = |i: usize| {
        let c = &cells[i];
        let lb = lattice(c);
        (lb, part_u[&(lb, c.col)], part_v[&(lb, c.row)])
    };

    // Adjacency: same lattice block, side by side.
    let index: BTreeMap<(u32, u32), usize> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| ((c.row, c.col), i))
        .collect();
    let mut parent: Vec<usize> = (0..cells.len()).collect();
    let mut links = Vec::new();
    for (i, c) in cells.iter().enumerate() {
        for (dr, dc) in [(0u32, 1u32), (1, 0)] {
            let Some(&j) = index.get(&(c.row + dr, c.col + dc)) else {
                continue;
            };
            if lattice(c) != lattice(&cells[j]) {
                continue;
            }
            let crosses = unit(i) != unit(j);
            links.push(CellLink {
                a: i,
                b: j,
                crosses_expansion_joint: crosses,
            });
            if !crosses {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri.max(rj)] = ri.min(rj);
                }
            }
        }
    }

    // Blocks, numbered in order of their first cell.
    let mut block_of = vec![usize::MAX; cells.len()];
    let mut blocks: Vec<MountingBlock> = Vec::new();
    let mut root_to_block: BTreeMap<usize, usize> = BTreeMap::new();
    for i in 0..cells.len() {
        let r = find(&mut parent, i);
        let id = *root_to_block.entry(r).or_insert_with(|| {
            blocks.push(MountingBlock {
                id: blocks.len(),
                cells: Vec::new(),
                lattice_block: lattice(&cells[i]),
                cols: 0,
                rows: 0,
                lo: Vec2::new(f64::INFINITY, f64::INFINITY),
                hi: Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            });
            blocks.len() - 1
        });
        block_of[i] = id;
        let b = &mut blocks[id];
        b.cells.push(i);
        let c = cells[i].centre;
        b.lo = Vec2::new(b.lo.u.min(c.u - hu), b.lo.v.min(c.v - hv));
        b.hi = Vec2::new(b.hi.u.max(c.u + hu), b.hi.v.max(c.v + hv));
    }
    for b in &mut blocks {
        let mut cols: Vec<u32> = b.cells.iter().map(|&i| cells[i].col).collect();
        let mut rows: Vec<u32> = b.cells.iter().map(|&i| cells[i].row).collect();
        cols.sort_unstable();
        cols.dedup();
        rows.sort_unstable();
        rows.dedup();
        b.cols = cols.len() as u32;
        b.rows = rows.len() as u32;
    }
    Ok(Structure {
        blocks,
        block_of,
        links,
        joints,
    })
}
