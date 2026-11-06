//! Syncing the project: idempotent, and the fast path (`sync_changes`) reaches the same
//! state as a full sync.

mod common;

use common::*;
use pv_graph::CancelToken;
use pv_model::{ChangeSet, ModuleId, ModuleState};
use pv_pipeline::{new_graph, sync_changes, sync_project, FieldStatics};

#[test]
fn full_sync_is_idempotent() {
    let p = ref_a_roof1();
    let mut g = graph_for(&p);
    let rev = g.revision();
    let r = sync_project(&mut g, &p).unwrap();
    assert_eq!(r.inputs_changed, 0);
    assert_eq!(r.forgotten, 0);
    assert_eq!(g.revision(), rev);
    assert!(g.run_auto(&CancelToken::new()).unwrap().is_empty());
}

#[test]
fn incremental_sync_equals_full_sync() {
    let mut p = ref_a_roof1();
    let mut incremental = graph_for(&p);
    // A sequence of module commands: moves, removals, restores.
    let n = p.modules.len() as u32;
    for step in 0..12u32 {
        let id = ModuleId((step * 37 + 5) % n);
        let m = &mut p.modules[id.0 as usize];
        match step % 3 {
            0 => m.u += 0.002,
            1 => m.state = ModuleState::Removed,
            _ => m.v -= 0.003,
        }
        sync_changes(
            &mut incremental,
            &p,
            &ChangeSet {
                module_ids: vec![id],
            },
        )
        .unwrap();
        if step % 4 == 0 {
            incremental.run_auto(&CancelToken::new()).unwrap();
        }
    }
    incremental.run_auto(&CancelToken::new()).unwrap();

    let mut full = new_graph();
    sync_project(&mut full, &p).unwrap();
    full.run_auto(&CancelToken::new()).unwrap();
    for f in 0..p.arrays.len() {
        let a = array(f, &p);
        assert_eq!(
            incremental.get::<FieldStatics>(a).unwrap(),
            full.get::<FieldStatics>(a).unwrap(),
            "field {}",
            f + 1
        );
    }
    // And a full sync on the incremental graph has nothing left to set.
    assert_eq!(
        sync_project(&mut incremental, &p).unwrap().inputs_changed,
        0
    );
}

#[test]
fn missing_site_and_mounting_are_reported_as_i18n_errors() {
    let mut p = ref_a_roof1();
    p.buildings[0].site = None;
    p.arrays[1].mounting = None;
    let mut g = graph_for(&p);
    let err = |r: Result<_, pv_graph::GraphError>| match r {
        Err(pv_graph::GraphError::Failed { error, .. }) => error.key,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        err(g.get::<FieldStatics>(array(0, &p)).map(|_| ())),
        "errors.statics.siteMissing"
    );
    assert_eq!(
        err(g.get::<FieldStatics>(array(1, &p)).map(|_| ())),
        "errors.statics.mountingMissing"
    );
    // Filling the site heals field 1.
    p.buildings[0].site = Some(site_of("ref-a").0);
    sync_project(&mut g, &p).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert!(g.get::<FieldStatics>(array(0, &p)).is_ok());
}
