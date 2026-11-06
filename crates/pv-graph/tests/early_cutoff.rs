//! A recomputed node with an unchanged output does not recompute its dependents.

mod common;

use common::*;
use pv_graph::{CancelToken, Instance, NodeStatus};

#[test]
fn same_output_stops_the_cascade() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.run_auto(&CancelToken::new()).unwrap();
    g.reset_stats();

    // half[1] = 5 div 4 = 1, as before: half[1] recomputes, nothing downstream does.
    g.set_input::<X>(i(1), 5).unwrap();
    assert_eq!(g.status_of::<Sum>(Instance::Project), NodeStatus::Stale);
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    let s = g.stats();
    assert_eq!(s.computed("half"), 1);
    assert_eq!(s.computed("pick"), 0);
    assert_eq!(s.computed("checked"), 0);
    assert_eq!(s.computed("sum"), 0);
    // pick[0], pick[1], checked[0], checked[1], sum confirmed without recomputation.
    assert_eq!(s.verified, 5);
    assert!(ready.is_empty(), "unchanged result is not re-emitted");
    assert_eq!(g.status_of::<Sum>(Instance::Project), NodeStatus::Fresh);
    assert_eq!(value::<Sum>(&mut g, Instance::Project), Ok(12));
}

#[test]
fn cut_off_in_the_middle_of_the_chain() {
    // sel odd: pick = 2x, checked = pick or error. x[0]: -1 → -2: pick changes (-2 → -4),
    // checked fails with another value, sum fails again with another error.
    let mut g = toy_with([-1, 4, 8, 12], 1);
    g.run_auto(&CancelToken::new()).unwrap();
    let before = value::<Sum>(&mut g, Instance::Project).unwrap_err();
    g.reset_stats();
    g.set_input::<X>(i(0), -2).unwrap();
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    let after = value::<Sum>(&mut g, Instance::Project).unwrap_err();
    assert_ne!(before, after);
    assert_eq!(ready.len(), 1, "a different error is a different result");
    assert_eq!(g.stats().computed("sum"), 1);
}
