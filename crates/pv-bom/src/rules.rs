//! Part rules × field counters → exact quantities.

use pv_model::{Driver, PartRule, RailLayerKind};

use crate::counts::FieldCounts;
use crate::cutting::joints;

/// Quantity of a counted part on one field; `None` for rail bars (cut, see
/// [`crate::cutting`]).
#[must_use]
pub fn counted(rule: &PartRule, c: &FieldCounts) -> Option<u64> {
    let n: u64 = match rule.driver {
        Driver::Fixation => c.fixations.into(),
        Driver::Climber => c.climbers.into(),
        Driver::TopRailEnd => 2 * c.clamp_rails.len() as u64,
        Driver::TopRailInnerJoint => c
            .clamp_rails
            .iter()
            .map(|&m| u64::from(m.saturating_sub(1)))
            .sum(),
        Driver::Module => c.modules.into(),
        Driver::BallastBlock => c.ballast_blocks.into(),
        Driver::PairRun { base, per_pair } => c
            .pair_runs
            .iter()
            .map(|&n| {
                u64::try_from(i64::from(base) + i64::from(per_pair) * i64::from(n)).unwrap_or(0)
            })
            .sum(),
        Driver::RailJoint {
            layer,
            stock_length_m,
        } => rail_pieces(layer, c)
            .iter()
            .map(|&l| joints(l, stock_length_m))
            .sum(),
        Driver::RailBars { .. } => return None,
    };
    Some(n * u64::from(rule.per))
}

/// Rail pieces of `layer` on one field [m].
#[must_use]
pub fn rail_pieces(layer: RailLayerKind, c: &FieldCounts) -> Vec<f64> {
    match layer {
        RailLayerKind::Top => c.top_rails.iter().map(|r| r.length_m).collect(),
        RailLayerKind::Bottom => c.bottom_rails.clone(),
    }
}
