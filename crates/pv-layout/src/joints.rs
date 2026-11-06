//! Expansion joints: where the rails of a mounting block are cut.
//!
//! Two sources:
//! - joints placed by the user ([`pv_model::ExpansionJointSpec`]): a line `u = x` cuts the
//!   rails running along `u` between the two columns whose centres straddle `x`;
//! - the mounting system's thermal rule: a block longer than `max_length` along a direction is
//!   split into the fewest parts that respect it, with the columns (rows) shared out as evenly
//!   as possible.
//!
//! Joints do not move modules: they cut rails inside the existing module gap.

use pv_model::{ExpansionJointSpec, JointAxis};
use serde::{Deserialize, Serialize};

/// Thermal rule of the mounting system and joints placed by hand.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JointRules {
    /// Maximum block length along `u` / `v` (module edge to module edge) [m].
    pub max_length_u: Option<f64>,
    pub max_length_v: Option<f64>,
    pub manual: Vec<ExpansionJointSpec>,
}

impl JointRules {
    #[must_use]
    pub fn from_params(p: &pv_model::ArrayLayoutParams) -> Self {
        Self {
            max_length_u: p.max_block_length_u_m,
            max_length_v: p.max_block_length_v_m,
            manual: p.expansion_joints.clone(),
        }
    }

    pub(crate) fn manual_positions(&self, axis: JointAxis) -> impl Iterator<Item = f64> + '_ {
        self.manual
            .iter()
            .filter(move |j| j.axis == axis)
            .map(|j| j.position_m)
    }
}

/// Origin of a joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JointSource {
    Manual,
    /// Inserted to respect the maximum block length.
    Thermal,
}

/// Part index of each line, and cuts as (last line before the cut, source).
pub(crate) type LineSplit = (Vec<usize>, Vec<(usize, JointSource)>);

/// Cuts along one direction of one lattice block.
///
/// `centres` are the distinct column (or row) centre coordinates of the lattice block, sorted
/// ascending. Returns, for each centre, the index of the part it belongs to, and the cuts as
/// `(index of the last centre before the cut, source)`. `None` when the rule cannot be met
/// because a single module is longer than `max_length`.
pub(crate) fn split_line(
    centres: &[f64],
    footprint: f64,
    max_length: Option<f64>,
    manual: &[f64],
) -> Option<LineSplit> {
    let n = centres.len();
    let mut cuts: Vec<(usize, JointSource)> = Vec::new();
    for &x in manual {
        if let Some(i) = (0..n.saturating_sub(1)).find(|&i| centres[i] < x && x < centres[i + 1]) {
            if !cuts.iter().any(|&(c, _)| c == i) {
                cuts.push((i, JointSource::Manual));
            }
        }
    }
    cuts.sort_by_key(|&(c, _)| c);

    if let Some(max) = max_length {
        if footprint > max + 1e-9 {
            return None;
        }
        // Segments between manual cuts, each split evenly if too long.
        let mut bounds: Vec<(usize, usize)> = Vec::new();
        let mut start = 0;
        for &(c, _) in &cuts {
            bounds.push((start, c));
            start = c + 1;
        }
        if n > 0 {
            bounds.push((start, n - 1));
        }
        let mut thermal = Vec::new();
        for (a, b) in bounds {
            let len = |i: usize, j: usize| centres[j] - centres[i] + footprint;
            if len(a, b) <= max + 1e-9 {
                continue;
            }
            let count = b - a + 1;
            let mut parts = 2;
            loop {
                // Even shares: the first `count % parts` parts get one more column.
                let (q, r) = (count / parts, count % parts);
                let mut edges = Vec::new();
                let mut i = a;
                let mut ok = true;
                for p in 0..parts {
                    let size = q + usize::from(p < r);
                    let j = i + size - 1;
                    ok &= len(i, j) <= max + 1e-9;
                    if p + 1 < parts {
                        edges.push(j);
                    }
                    i = j + 1;
                }
                if ok {
                    thermal.extend(edges.into_iter().map(|e| (e, JointSource::Thermal)));
                    break;
                }
                parts += 1;
                if parts > count {
                    return None;
                }
            }
        }
        cuts.extend(thermal);
        cuts.sort_by_key(|&(c, _)| c);
    }

    let mut part = vec![0usize; n];
    let mut k = 0;
    for (i, slot) in part.iter_mut().enumerate() {
        *slot = k;
        if cuts.iter().any(|&(c, _)| c == i) {
            k += 1;
        }
    }
    Some((part, cuts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centres(n: usize, pitch: f64) -> Vec<f64> {
        (0..n).map(|i| i as f64 * pitch).collect()
    }

    #[test]
    fn no_rule_no_cut() {
        let (part, cuts) = split_line(&centres(6, 1.767), 1.757, None, &[]).unwrap();
        assert!(cuts.is_empty());
        assert!(part.iter().all(|&p| p == 0));
    }

    #[test]
    fn thermal_rule_splits_evenly() {
        // 7 modules of 1.0 m at 1.1 m pitch = 7.6 m; max 4 m → 2 parts of 4 + 3 (4.3 m > 4) → 3 parts.
        let (part, cuts) = split_line(&centres(7, 1.1), 1.0, Some(4.0), &[]).unwrap();
        assert_eq!(part, vec![0, 0, 0, 1, 1, 2, 2]);
        assert_eq!(cuts.len(), 2);
        assert!(cuts.iter().all(|&(_, s)| s == JointSource::Thermal));
    }

    #[test]
    fn manual_joint_then_thermal() {
        let c = centres(10, 1.0);
        let (part, cuts) = split_line(&c, 0.9, Some(4.0), &[2.5]).unwrap();
        // Manual cut after column 2; the right segment (7 columns, 6.9 m) is split in 2.
        assert_eq!(cuts[0], (2, JointSource::Manual));
        assert_eq!(part, vec![0, 0, 0, 1, 1, 1, 1, 2, 2, 2]);
    }

    #[test]
    fn module_longer_than_rule_is_reported() {
        assert!(split_line(&centres(3, 2.0), 1.9, Some(1.5), &[]).is_none());
    }
}
