//! Forgetting a deleted entity: its nodes are released, readers on other instances recompute,
//! and slots are reused (no leak over repeated create / delete cycles).

mod common;

use common::*;
use pv_graph::{CancelToken, GraphError, Instance, Kind, NodeStatus};

#[test]
fn forgotten_nodes_are_released_and_readers_recompute() {
    let mut g = toy_with([0, 4, 8, 12], 1); // sel odd: pick[i] = 2·x[i]
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(value::<Sum>(&mut g, Instance::Project), Ok(48));
    let live = g.node_count();

    // x, pick, checked of instance #1 (half[1] was never computed with sel odd).
    assert_eq!(g.forget(i(1)), 3);
    assert_eq!(g.node_count(), live - 3);
    assert_eq!(g.status_of::<Pick>(i(1)), NodeStatus::Missing);
    assert_eq!(g.status_of::<Sum>(Instance::Project), NodeStatus::Stale);

    // sum reads checked[1] again: recreated, and x[1] is gone.
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert!(ready[0].failed);
    assert_eq!(
        value::<Sum>(&mut g, Instance::Project),
        Err(GraphError::MissingInput(X::key(i(1))))
    );
    // Recreating the entity heals the reader.
    g.set_input::<X>(i(1), 10).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(
        value::<Sum>(&mut g, Instance::Project),
        Ok(2 * (10 + 8 + 12)) // x = [0, 10, 8, 12]
    );
    assert_eq!(g.forget(i(99)), 0);
}

#[test]
fn repeated_create_delete_cycles_do_not_leak() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.run_auto(&CancelToken::new()).unwrap();
    let mut capacity = None;
    for round in 0..200 {
        g.forget(i(2));
        g.set_input::<X>(i(2), round).unwrap();
        g.run_auto(&CancelToken::new()).unwrap();
        let expected = {
            let mut fresh = toy_with([0, 4, round, 12], 0);
            value::<Sum>(&mut fresh, Instance::Project)
        };
        assert_eq!(value::<Sum>(&mut g, Instance::Project), expected);
        // After the first cycle the arena stops growing.
        match capacity {
            None if round > 0 => capacity = Some(g.slot_capacity()),
            Some(c) => assert_eq!(g.slot_capacity(), c, "round {round}"),
            None => {}
        }
    }
}

#[test]
fn forgetting_a_watched_node_unwatches_it() {
    let mut g = toy_with([0, 4, 8, 12], 0);
    g.watch::<Pick>(i(3)).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    g.forget(i(3));
    assert!(!g.watched().contains(&Pick::key(i(3))));
    // The pass skips it; sum recomputes and fails on the missing x[3].
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert!(ready.iter().all(|r| r.key != Pick::key(i(3))));
}
