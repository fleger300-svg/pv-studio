//! A node that reads different nodes from one computation to the next.

mod common;

use common::*;
use pv_graph::{CancelToken, Instance, Kind, NodeStatus};

#[test]
fn dependencies_follow_the_last_computation() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(
        g.dependencies(Pick::key(i(0))),
        &[
            Sel::key(Instance::Project),
            Half::key(i(0)),
            Half::key(i(1))
        ]
    );
    assert!(g.dependents(Half::key(i(1))).contains(&Pick::key(i(0))));

    // sel odd: pick reads x directly; the old edges are dropped.
    g.set_input::<Sel>(Instance::Project, 1).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(
        g.dependencies(Pick::key(i(0))),
        &[Sel::key(Instance::Project), X::key(i(0))]
    );
    assert!(g.dependents(Half::key(i(1))).is_empty());
    assert_eq!(
        value::<Sum>(&mut g, Instance::Project),
        Ok(2 * (4 + 8 + 12))
    );

    // half is no longer read by anything watched: changing x[1] recomputes pick[1] only.
    g.reset_stats();
    g.set_input::<X>(i(1), 40).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(g.stats().computed("half"), 0);
    assert_eq!(g.stats().computed("pick"), 1);
    assert_eq!(g.status_of::<Half>(i(1)), NodeStatus::Stale);
    assert_eq!(
        value::<Sum>(&mut g, Instance::Project),
        Ok(2 * (40 + 8 + 12))
    );

    // Back to sel even: half edges come back, half[1] is recomputed with the new x.
    g.set_input::<Sel>(Instance::Project, 2).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert!(g.dependents(Half::key(i(1))).contains(&Pick::key(i(0))));
    // halves 0,10,2,3 → picks 10,12,5,3 → 30
    assert_eq!(value::<Sum>(&mut g, Instance::Project), Ok(30));
}
