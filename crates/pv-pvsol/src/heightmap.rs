//! `HeightMap.bin`: PV*SOL's digital surface model of the map section (terrain, buildings,
//! trees), used by PV*SOL for the shading by the surroundings.
//!
//! Layout (little endian): `u16` columns (west → east), `u16` rows (north → south), `f32`
//! base altitude [m], then `u16` heights above the base [cm], **column by column** (index
//! `col × rows + row`). Checked on project A against `HeightMap.jpg` / `MapExtract.jpg`
//! (river bed and the two highest roofs) and on the scale: 2000 × 1999 cells for a
//! 200 × 199.9 m map section.
//!
//! Vertically, the scene's `z = 0` is the altitude of the surface at the scene origin (on
//! project A the building's foot, `z = −1.86 m`, then lies at street level).

use pv_model::Surroundings;

use crate::geometry::MapSection;
use crate::ReadError;

/// A decoded height map.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightMap {
    pub cols: usize,
    pub rows: usize,
    pub base_m: f64,
    /// Above the base [cm], row-major from the north-west corner.
    pub cm: Vec<u16>,
}

impl HeightMap {
    /// Absolute altitude [m] of a cell.
    #[must_use]
    pub fn altitude(&self, row: usize, col: usize) -> f64 {
        self.base_m + f64::from(self.cm[row * self.cols + col]) / 100.0
    }
}

/// Decodes `HeightMap.bin`.
///
/// # Errors
/// [`ReadError::Format`] when the size does not match the header.
pub fn parse(b: &[u8]) -> Result<HeightMap, ReadError> {
    let bad = || ReadError::Format {
        file: "HeightMap.bin".into(),
        what: "size".into(),
    };
    let cols = usize::from(u16::from_le_bytes(
        b.get(0..2).ok_or_else(bad)?.try_into().map_err(|_| bad())?,
    ));
    let rows = usize::from(u16::from_le_bytes(
        b.get(2..4).ok_or_else(bad)?.try_into().map_err(|_| bad())?,
    ));
    let base = f32::from_le_bytes(b.get(4..8).ok_or_else(bad)?.try_into().map_err(|_| bad())?);
    if cols == 0 || rows == 0 || b.len() != 8 + 2 * cols * rows || !base.is_finite() {
        return Err(bad());
    }
    let mut cm = vec![0u16; cols * rows];
    for c in 0..cols {
        for r in 0..rows {
            let i = 8 + 2 * (c * rows + r);
            cm[r * cols + c] = u16::from_le_bytes([b[i], b[i + 1]]);
        }
    }
    Ok(HeightMap {
        cols,
        rows,
        base_m: f64::from(base),
        cm,
    })
}

/// Cell size of the surroundings kept in the project [m] (heights: maximum per cell).
pub const CELL_M: f64 = 0.5;

/// Height above the local ground from which a surveyed cell belongs to a building [m].
const BUILDING_ABOVE_GROUND_M: f64 = 2.5;
/// Radius around the modules within which the project's own building is searched [m].
const OWN_SEARCH_RADIUS_M: f64 = 80.0;
/// Width of the ring around the cut-out where the ground level is read [m].
const GROUND_RING_M: f64 = 5.0;
/// The survey under the modules standing higher above them than this means the map's base is
/// not the scene's zero [m].
const MAX_ROOF_GAP_M: f64 = 2.0;
/// How far around the footprints of the project's buildings the survey's building is still
/// theirs [m].
const FOOTPRINT_GROWTH_M: f64 = 6.0;
/// Faces steeper than this are facades: the survey under their modules is not their roof.
const FACADE_TILT_DEG: f64 = 60.0;

/// Chamfer distance (in cells, 3-4 weights / 3) from every cell to the nearest `true` cell.
fn distance(mask: &[bool], cols: usize, rows: usize) -> Vec<f32> {
    let mut d: Vec<f32> = mask
        .iter()
        .map(|&m| if m { 0.0 } else { f32::MAX })
        .collect();
    let (a, b) = (1.0f32, 4.0f32 / 3.0);
    for r in 0..rows {
        for c in 0..cols {
            let i = r * cols + c;
            let mut v = d[i];
            if c > 0 {
                v = v.min(d[i - 1] + a);
            }
            if r > 0 {
                v = v.min(d[i - cols] + a);
                if c > 0 {
                    v = v.min(d[i - cols - 1] + b);
                }
                if c + 1 < cols {
                    v = v.min(d[i - cols + 1] + b);
                }
            }
            d[i] = v;
        }
    }
    for r in (0..rows).rev() {
        for c in (0..cols).rev() {
            let i = r * cols + c;
            let mut v = d[i];
            if c + 1 < cols {
                v = v.min(d[i + 1] + a);
            }
            if r + 1 < rows {
                v = v.min(d[i + cols] + a);
                if c + 1 < cols {
                    v = v.min(d[i + cols + 1] + b);
                }
                if c > 0 {
                    v = v.min(d[i + cols - 1] + b);
                }
            }
            d[i] = v;
        }
    }
    d
}

fn percentile(mut v: Vec<f64>, p: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let i = ((v.len() - 1) as f64 * p).round() as usize;
    v.get(i).copied()
}

/// The height map as the project's surroundings, in the scene frame.
///
/// - **Own building cut out**, as PV*SOL does (`GelaendeModell/DoCutOuts`, `AddDist`): the
///   cells under the `footprints` of the buildings carrying the modules, and the cells
///   connected to the modules higher than the local ground by [`BUILDING_ABOVE_GROUND_M`]
///   (whatever the survey shows there: its roof, or what stood before) within
///   [`FOOTPRINT_GROWTH_M`] of the footprints, removed with `margin_m` around them. Without
///   footprints, all the connected cells (a tree touching the building goes with it).
/// - **Vertically**, the scene's zero is the map's base (its lowest point; checked on the
///   corpus: the survey's roofs then stand at the modules' height), unless the survey under
///   the modules then stands more than [`MAX_ROOF_GAP_M`] above them; the ground read around
///   the cut-out (10th percentile of a [`GROUND_RING_M`] ring) is then the foot of the
///   building, `scene_ground_m` in the scene.
///
/// `modules`: centres `[east, north, up]` and the tilt of their face [°].
///
/// `None` when the map section is not the usual one (rotated by 180°), does not match the
/// grid, or no module lies on it.
#[must_use]
pub fn surroundings(
    h: &HeightMap,
    map: &MapSection,
    modules: &[[f64; 4]],
    footprints: &[Vec<[f64; 2]>],
    scene_ground_m: f64,
    margin_m: f64,
) -> Option<Surroundings> {
    if ((map.rotation_deg - 180.0).abs() > 0.01) || map.width_m <= 0.0 || map.depth_m <= 0.0 {
        return None;
    }
    let cell = map.width_m / h.cols as f64;
    if ((map.depth_m / h.rows as f64) - cell).abs() > 1e-3 * cell {
        return None;
    }
    let (cols, rows) = (h.cols, h.rows);
    // Position corner = south-west; the grid starts at the north-west.
    let west = map.corner[0];
    let north = map.corner[1] + map.depth_m;
    let index = |e: f64, n: f64| -> Option<(usize, usize)> {
        let c = ((e - west) / cell).floor();
        let r = ((north - n) / cell).floor();
        (c >= 0.0 && r >= 0.0 && (c as usize) < cols && (r as usize) < rows)
            .then_some((r as usize, c as usize))
    };
    let seeds: Vec<(usize, usize)> = modules.iter().filter_map(|m| index(m[0], m[1])).collect();
    if seeds.is_empty() {
        return None;
    }
    let n = seeds.len() as f64;
    let (cr, cc) = (
        seeds.iter().map(|s| s.0 as f64).sum::<f64>() / n,
        seeds.iter().map(|s| s.1 as f64).sum::<f64>() / n,
    );
    let radius = OWN_SEARCH_RADIUS_M / cell;
    let near = |r: usize, c: usize| (r as f64 - cr).hypot(c as f64 - cc) <= radius;

    // Local ground: low percentile of the surface around the modules.
    let mut around = Vec::new();
    let step = (1.0 / cell).round().max(1.0) as usize;
    for r in (0..rows).step_by(step) {
        for c in (0..cols).step_by(step) {
            if (r as f64 - cr).hypot(c as f64 - cc) <= 30.0 / cell {
                around.push(h.altitude(r, c));
            }
        }
    }
    let ground = percentile(around, 0.05)?;

    // The project's buildings: under their footprints when known, else the cells connected
    // to the modules, above the ground.
    let mut own = vec![false; cols * rows];
    if !footprints.is_empty() {
        for r in 0..rows {
            for c in 0..cols {
                let p = [
                    west + (c as f64 + 0.5) * cell,
                    north - (r as f64 + 0.5) * cell,
                ];
                if footprints
                    .iter()
                    .any(|f| crate::import::point_in_polygon(f, p))
                {
                    own[r * cols + c] = true;
                }
            }
        }
    }
    // Plus the cells connected to the modules, above the ground: within
    // [`FOOTPRINT_GROWTH_M`] of the footprints (the survey's building may be larger than the
    // model: annexes, attached houses), anywhere when no footprint is known.
    let grow = (FOOTPRINT_GROWTH_M / cell) as f32;
    let fp_dist = (!footprints.is_empty()).then(|| distance(&own, cols, rows));
    let allowed = |j: usize| fp_dist.as_ref().is_none_or(|d| d[j] <= grow);
    let mut flood = vec![false; cols * rows];
    let mut stack = seeds;
    while let Some((r, c)) = stack.pop() {
        let i = r * cols + c;
        if flood[i] {
            continue;
        }
        flood[i] = true;
        own[i] = true;
        for (dr, dc) in [
            (-1i64, 0i64),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let (nr, nc) = (r as i64 + dr, c as i64 + dc);
            if nr < 0 || nc < 0 || nr >= rows as i64 || nc >= cols as i64 {
                continue;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            let j = nr * cols + nc;
            let inside = fp_dist.as_ref().is_some_and(|d| d[j] <= 0.0);
            if !flood[j]
                && near(nr, nc)
                && allowed(j)
                && (inside || h.altitude(nr, nc) >= ground + BUILDING_ABOVE_GROUND_M)
            {
                stack.push((nr, nc));
            }
        }
    }
    let dist = distance(&own, cols, rows);
    let cut_cells = (margin_m / cell) as f32;
    let ring_cells = ((margin_m + GROUND_RING_M) / cell) as f32;
    let ring: Vec<f64> = (0..rows * cols)
        .filter(|&i| dist[i] > cut_cells && dist[i] <= ring_cells)
        .map(|i| h.altitude(i / cols, i % cols))
        .collect();
    // The scene's zero is the map's base, unless the survey under the (non-facade) modules
    // then stands above them: the building is not placed on the survey's terrain; its foot
    // is then the ground read around the cut-out.
    let roof_gap = percentile(
        modules
            .iter()
            .filter(|m| m[3] < FACADE_TILT_DEG)
            .filter_map(|m| index(m[0], m[1]).map(|(r, c)| h.altitude(r, c) - h.base_m - m[2]))
            .collect(),
        0.5,
    );
    let zero = if roof_gap.is_some_and(|g| g > MAX_ROOF_GAP_M) {
        percentile(ring, 0.10)? - scene_ground_m
    } else {
        h.base_m
    };

    let k = (CELL_M / cell).round().max(1.0) as usize;
    let (out_cols, out_rows) = (cols.div_ceil(k), rows.div_ceil(k));
    let mut heights = Vec::with_capacity(out_cols * out_rows);
    for r in 0..out_rows {
        for c in 0..out_cols {
            let mut top = f64::NEG_INFINITY;
            for rr in r * k..((r + 1) * k).min(rows) {
                for cc in c * k..((c + 1) * k).min(cols) {
                    if dist[rr * cols + cc] > cut_cells {
                        top = top.max(h.altitude(rr, cc));
                    }
                }
            }
            heights.push(if top.is_finite() {
                ((top - zero) * 100.0).round() as i32
            } else {
                Surroundings::CUT
            });
        }
    }
    Some(Surroundings {
        source: "PV*SOL (modèle de surface du projet)".to_owned(),
        origin_east_m: west,
        origin_north_m: north,
        cell_m: cell * k as f64,
        cols: u32::try_from(out_cols).ok()?,
        rows: u32::try_from(out_rows).ok()?,
        heights_cm: heights,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 × 3 map (0.1 m cells) with one tall cell at row 0, column 3 (north-east).
    fn sample() -> Vec<u8> {
        let (cols, rows) = (4u16, 3u16);
        let mut b = Vec::new();
        b.extend_from_slice(&cols.to_le_bytes());
        b.extend_from_slice(&rows.to_le_bytes());
        b.extend_from_slice(&100.0f32.to_le_bytes());
        for c in 0..cols {
            for r in 0..rows {
                let v: u16 = if (r, c) == (0, 3) { 500 } else { 100 };
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn reads_column_by_column() {
        let h = parse(&sample()).unwrap();
        assert_eq!((h.cols, h.rows), (4, 3));
        assert_eq!(h.altitude(0, 3), 105.0);
        assert_eq!(h.altitude(2, 0), 101.0);
        assert!(parse(&sample()[..20]).is_err());
    }

    #[test]
    fn cuts_the_own_building_and_keeps_the_neighbours() {
        // 10 × 10 m at 0.1 m, ground at 100 m; the project's building (cells 40..60, +6 m)
        // under a module at the scene origin; a neighbour in the north-east corner (+10 m).
        let (cols, rows) = (100usize, 100usize);
        let mut cm = vec![0u16; cols * rows];
        for r in 0..rows {
            for c in 0..cols {
                if (40..60).contains(&r) && (40..60).contains(&c) {
                    cm[r * cols + c] = 600;
                } else if r < 10 && c >= 80 {
                    cm[r * cols + c] = 1000;
                }
            }
        }
        let h = HeightMap {
            cols,
            rows,
            base_m: 100.0,
            cm,
        };
        let map = MapSection {
            corner: [-5.0, -5.0],
            width_m: 10.0,
            depth_m: 10.0,
            rotation_deg: 180.0,
        };
        // The module stands on the survey's roof (6 m): the scene's zero is the map's base.
        let s = surroundings(&h, &map, &[[0.0, 0.0, 6.2, 30.0]], &[], -1.0, 1.0).unwrap();
        assert_eq!((s.cols, s.rows), (20, 20));
        let cell = |r: usize, c: usize| s.heights_cm[r * 20 + c];
        assert_eq!(cell(10, 10), Surroundings::CUT, "own building cut out");
        assert_eq!(cell(7, 10), Surroundings::CUT, "within the 1 m margin");
        assert_eq!(cell(0, 19), 1000, "neighbour kept, 10 m above the base");
        assert_eq!(cell(19, 0), 0, "ground at the base");
        // The module stands 4 m under the survey's roof: the building is not placed on the
        // survey's terrain; its foot (z = −1 in the scene) is the ground around it.
        let s = surroundings(&h, &map, &[[0.0, 0.0, 2.0, 30.0]], &[], -1.0, 1.0).unwrap();
        let cell = |r: usize, c: usize| s.heights_cm[r * 20 + c];
        assert_eq!(cell(0, 19), 900, "neighbour 10 m above a ground at z = −1");
        assert_eq!(cell(19, 0), -100, "ground");
        // With the building's footprint: the cut follows it (and the survey's building
        // around it), the neighbour 3 m away stays.
        let fp = vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let s = surroundings(&h, &map, &[[0.0, 0.0, 6.2, 30.0]], &[fp], -1.0, 1.0).unwrap();
        let cell = |r: usize, c: usize| s.heights_cm[r * 20 + c];
        assert_eq!(cell(10, 10), Surroundings::CUT, "footprint cut out");
        assert_eq!(cell(0, 19), 1000, "neighbour kept");
        assert!(surroundings(&h, &map, &[], &[], 0.0, 1.0).is_none());
    }
}
