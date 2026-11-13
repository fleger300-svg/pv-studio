//! L3: a one-layer rail field on rafters (K2 SingleRail on tile hooks) through the graph:
//! hooks on every n-th rafter as the statics allow, no top-rail check, clamps counted on the
//! module-carrying rails.

mod common;

use common::*;
use pv_model::{Driver, RailLayers, SupportGridKind, SupportGridSpec};
use pv_pipeline::nodes::bom::FieldCountsNode;
use pv_pipeline::nodes::descent::{Descent, FixationEvery};
use pv_pipeline::nodes::layout::Hardware;

const RAFTERS_M: f64 = 0.6;

fn one_layer_on_rafters() -> pv_model::Project {
    let mut p = ref_a_roof1();
    for f in &mut p.roof_faces {
        f.support_grid = Some(SupportGridSpec {
            kind: SupportGridKind::Rafters,
            origin_m: 0.1,
            spacing_m: RAFTERS_M,
        });
    }
    for a in &mut p.arrays {
        let mut r = a.layout.rails.expect("rail system");
        r.layers = RailLayers::One;
        r.per_column = 2;
        r.top_inset_m = 0.25;
        r.max_cantilever_m = None;
        a.layout.rails = Some(r);
    }
    p
}

#[test]
fn hooks_follow_the_statics_spacing() {
    let p = one_layer_on_rafters();
    let mut g = graph_for(&p);
    let i = array(0, &p);

    let every = *g.get::<FixationEvery>(i).unwrap();
    assert!(every >= 1);
    let descent = g.get::<Descent>(i).unwrap();
    let zones = descent.as_ref().as_ref().expect("rail system");
    assert!(zones.iter().all(|z| z.result.top_rail.is_none()));
    // The governing zone's spacing is the one laid out.
    let governing = zones
        .iter()
        .map(|z| z.result.fixation_spacing.value)
        .fold(f64::INFINITY, f64::min);
    assert!((governing - f64::from(every) * RAFTERS_M).abs() < 1e-9);

    let hw = g.get::<Hardware>(i).unwrap();
    let hw = hw.as_ref().as_ref().expect("hardware");
    assert!(hw.rails.top.is_empty() && hw.rails.climbers.is_empty());
    assert_eq!(hw.rails.bottom.len(), hw.rails.bottom_modules.len());
    let fx = hw.fixations.as_ref().expect("rafter grid");
    for pos in &fx.positions {
        assert!(pos.len() >= 2);
        for w in pos.windows(2) {
            let gap = w[1] - w[0];
            let k = gap / RAFTERS_M;
            assert!((k - k.round()).abs() < 1e-6, "gap {gap} on the rafters");
            assert!(gap <= f64::from(every) * RAFTERS_M + 1e-9, "gap {gap}");
        }
    }
}

#[test]
fn clamps_are_counted_on_the_carrying_rails() {
    let p = one_layer_on_rafters();
    let mut g = graph_for(&p);
    let i = array(0, &p);
    let c = g.get::<FieldCountsNode>(i).unwrap();
    let hw = g.get::<Hardware>(i).unwrap();
    let hw = hw.as_ref().as_ref().expect("hardware");
    assert_eq!(c.clamp_rails, hw.rails.bottom_modules);
    assert!(c.top_rails.is_empty() && c.climbers == 0);
    let rule = |d: Driver| pv_model::PartRule {
        article: c.parts[0].article.clone(),
        driver: d,
        per: 1,
    };
    let ec = pv_bom::rules::counted(&rule(Driver::TopRailEnd), &c).unwrap();
    let mc = pv_bom::rules::counted(&rule(Driver::TopRailInnerJoint), &c).unwrap();
    let modules: u64 = hw.rails.bottom_modules.iter().map(|&m| u64::from(m)).sum();
    let rails = hw.rails.bottom_modules.len() as u64;
    // K2: EC = 2 × rails, MC = Σ(modules − 1) over the rails.
    assert_eq!(ec, 2 * rails);
    assert_eq!(mc, modules - rails);
}
