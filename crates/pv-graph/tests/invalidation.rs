//! Only the dependents of a changed input are recomputed (chain, diamond, fan-in).

mod common;

use common::*;
use pv_graph::{CancelToken, Instance, Kind, NodeStatus};

#[test]
fn first_pass_computes_each_node_once() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    let s = g.stats();
    // Diamond: half[i] is read by pick[i] and pick[i-1], computed once.
    assert_eq!(s.computed("half"), 4);
    assert_eq!(s.computed("pick"), 4);
    assert_eq!(s.computed("checked"), 4);
    assert_eq!(s.computed("sum"), 1);
    assert_eq!(s.computed("report"), 0, "on-demand, not watched");
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].key, Sum::key(Instance::Project));
    assert!(!ready[0].failed);
    // halves 0,1,2,3 → picks 1,3,5,3 → sum 12
    assert_eq!(value::<Sum>(&mut g, Instance::Project), Ok(12));

    // A second pass has nothing to do and nothing to report.
    g.reset_stats();
    assert!(g.run_auto(&CancelToken::new()).unwrap().is_empty());
    assert_eq!(g.stats().total_computed(), 0);
}

#[test]
fn a_change_recomputes_only_its_dependents() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    let first = g.run_auto(&CancelToken::new()).unwrap();
    g.reset_stats();

    assert!(g.set_input::<X>(i(0), 8).unwrap()); // half[0]: 0 → 2
                                                 // Stale: half[0], pick[0], pick[3] (reads half[0]), their checked, sum. Not pick[1], pick[2].
    assert_eq!(g.status_of::<Pick>(i(0)), NodeStatus::Stale);
    assert_eq!(g.status_of::<Pick>(i(3)), NodeStatus::Stale);
    assert_eq!(g.status_of::<Pick>(i(1)), NodeStatus::Fresh);
    assert_eq!(g.status_of::<Checked>(i(2)), NodeStatus::Fresh);
    assert_eq!(g.stats().dirtied, 6);

    let ready = g.run_auto(&CancelToken::new()).unwrap();
    let s = g.stats();
    assert_eq!(s.computed("half"), 1);
    assert_eq!(s.computed("pick"), 2);
    assert_eq!(s.computed("checked"), 2);
    assert_eq!(s.computed("sum"), 1);
    assert_eq!(value::<Sum>(&mut g, Instance::Project), Ok(16));
    assert_eq!(ready.len(), 1);
    assert!(ready[0].revision > first[0].revision);
    assert_ne!(ready[0].input_fingerprint, first[0].input_fingerprint);
    assert_ne!(ready[0].output_fingerprint, first[0].output_fingerprint);
}

#[test]
fn setting_the_same_value_changes_nothing() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.run_auto(&CancelToken::new()).unwrap();
    let rev = g.revision();
    assert!(!g.set_input::<X>(i(2), 8).unwrap());
    assert_eq!(g.revision(), rev);
    assert_eq!(g.status_of::<Sum>(Instance::Project), NodeStatus::Fresh);
}

#[test]
fn errors_are_results_and_heal() {
    let mut g = toy_with([0, 4, 8, 12], 1); // sel odd: pick = 2x
    g.run_auto(&CancelToken::new()).unwrap();
    g.set_input::<X>(i(1), -3).unwrap();
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert!(ready[0].failed);
    let err = value::<Sum>(&mut g, Instance::Project).unwrap_err();
    match &err {
        pv_graph::GraphError::Failed { origin, error } => {
            assert_eq!(*origin, Checked::key(i(1)), "origin kept downstream");
            assert_eq!(error.key, "TEST_NEGATIVE");
        }
        e => panic!("{e:?}"),
    }
    assert!(matches!(
        g.status_of::<Sum>(Instance::Project),
        NodeStatus::Failed(_)
    ));
    // Removing the input: missing input propagates.
    g.remove_input::<X>(i(1)).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(
        value::<Sum>(&mut g, Instance::Project),
        Err(pv_graph::GraphError::MissingInput(X::key(i(1))))
    );
    // Fixing it heals exactly what failed.
    g.reset_stats();
    g.set_input::<X>(i(1), 3).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(
        value::<Sum>(&mut g, Instance::Project),
        Ok(2 * (3 + 8 + 12))
    );
    assert_eq!(g.stats().computed("pick"), 1);
}

#[test]
fn reading_an_unset_input_records_the_dependency() {
    let mut g = toy_graph();
    g.set_input::<Sel>(Instance::Project, 1).unwrap();
    assert_eq!(
        value::<Pick>(&mut g, i(0)),
        Err(pv_graph::GraphError::MissingInput(X::key(i(0))))
    );
    g.set_input::<X>(i(0), 5).unwrap();
    assert_eq!(g.status_of::<Pick>(i(0)), NodeStatus::Stale);
    assert_eq!(value::<Pick>(&mut g, i(0)), Ok(10));
}

#[test]
fn misuse_is_reported() {
    let mut g = toy_graph();
    assert!(matches!(
        g.register::<Half>(),
        Err(pv_graph::GraphError::DuplicateKind(_))
    ));
    assert!(matches!(
        g.watch_key(X::key(i(0))),
        Err(pv_graph::GraphError::NotDerived(_))
    ));
    let mut empty = pv_graph::Graph::new();
    assert!(matches!(
        empty.get::<Half>(i(0)),
        Err(pv_graph::GraphError::UnknownKind(_))
    ));
}
