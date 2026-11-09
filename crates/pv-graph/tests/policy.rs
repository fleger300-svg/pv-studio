//! `OnDemand` nodes are only marked stale by passes; `Auto` nodes may not read them.

mod common;

use common::*;
use pv_graph::{
    serde_fingerprint, CancelToken, Ctx, Derived, GraphError, Instance, Kind, NodeStatus,
};

#[test]
fn on_demand_stays_stale_until_requested() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.watch::<Report>(Instance::Project).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(g.stats().computed("report"), 0);
    assert_eq!(
        g.status_of::<Report>(Instance::Project),
        NodeStatus::Missing
    );
    assert_eq!(g.stale(), vec![Report::key(Instance::Project)]);

    // Explicit request.
    assert_eq!(value::<Report>(&mut g, Instance::Project), Ok(120));
    assert!(g.stale().is_empty());
    // Reported once by the next pass.
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].key, Report::key(Instance::Project));

    // A real change: sum is recomputed automatically, report only marked stale.
    g.reset_stats();
    g.set_input::<X>(i(0), 8).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(g.stats().computed("sum"), 1);
    assert_eq!(g.stats().computed("report"), 0);
    assert_eq!(g.status_of::<Report>(Instance::Project), NodeStatus::Stale);
    assert_eq!(g.stale(), vec![Report::key(Instance::Project)]);
    assert_eq!(value::<Report>(&mut g, Instance::Project), Ok(160));
}

#[test]
fn on_demand_is_cleared_when_upstream_is_cut_off() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.watch::<Report>(Instance::Project).unwrap();
    value::<Report>(&mut g, Instance::Project).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    g.reset_stats();

    g.set_input::<X>(i(1), 6).unwrap(); // half[1] still 1
    assert_eq!(g.status_of::<Report>(Instance::Project), NodeStatus::Stale);
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert!(ready.is_empty());
    assert_eq!(g.stats().computed("report"), 0);
    assert_eq!(g.status_of::<Report>(Instance::Project), NodeStatus::Fresh);
}

struct Eager;
impl Kind for Eager {
    type Output = i64;
    const NAME: &'static str = "eager";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Eager {
    fn compute(ctx: &mut Ctx<'_>, _: Instance) -> Result<i64, GraphError> {
        Ok(*ctx.get::<Report>(Instance::Project)?)
    }
}

#[test]
fn auto_reading_on_demand_is_an_error() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.register::<Eager>().unwrap();
    g.watch::<Eager>(Instance::Project).unwrap();
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert!(ready.iter().any(|r| r.failed));
    assert_eq!(
        g.status_of::<Eager>(Instance::Project),
        NodeStatus::Failed(GraphError::AutoReadsOnDemand {
            reader: Eager::key(Instance::Project),
            dependency: Report::key(Instance::Project),
        })
    );
    assert_eq!(g.stats().computed("report"), 0);
}
