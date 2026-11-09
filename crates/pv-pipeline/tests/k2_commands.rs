//! G4 end to end: project A roof 1 reached through real commands (`ProjectStore`), then
//! computed incrementally, must still match the K2 Base report; and stay so through edits
//! that are undone and redone.

mod common;

use common::*;
use pv_graph::{CancelToken, Graph};
use pv_model::{ChangeSet, Command, ModuleId, ModuleState};
use pv_pipeline::{sync_changes, sync_project};
use pv_store::{Author, ProjectStore};

fn step(g: &mut Graph, store: &ProjectStore, changes: &ChangeSet) {
    sync_changes(g, store.project(), changes).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    compare_with_full(g, store.project()).unwrap();
}

fn assert_k2(g: &mut Graph, store: &ProjectStore, when: &str) {
    let (compared, failures) = k2::roof1_deviations(g, store.project());
    assert!(compared > 80, "{when}: {compared} values compared");
    assert!(failures.is_empty(), "{when}:\n{}", failures.join("\n"));
}

#[test]
fn k2_roof_1_reached_through_commands() {
    // Start with every module of both fields in place; K2's removed module comes by command.
    let mut base = ref_a_roof1();
    let removed: Vec<ModuleId> = base
        .modules
        .iter_mut()
        .filter(|m| m.state == ModuleState::Removed)
        .map(|m| {
            m.state = ModuleState::Active;
            m.id
        })
        .collect();
    assert_eq!(removed.len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_for(&dir, "k2", &base);
    let mut g = pv_pipeline::new_graph();
    sync_project(&mut g, store.project()).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();

    // 1. The module K2 leaves out is removed by command.
    let c = store
        .apply(
            &Command::RemoveModules {
                ids: removed.clone(),
            },
            Author::User,
        )
        .unwrap();
    step(&mut g, &store, &c);
    assert_k2(&mut g, &store, "after the removal");

    // 2. Four modules of field 1 nudged (inside the gaps), then undone.
    let field_1: Vec<ModuleId> = store
        .project()
        .modules
        .iter()
        .filter(|m| m.array_id == store.project().arrays[0].id)
        .map(|m| m.id)
        .take(4)
        .collect();
    let c = store
        .apply(
            &Command::MoveModules {
                ids: field_1,
                du: 0.004,
                dv: -0.006,
            },
            Author::User,
        )
        .unwrap();
    step(&mut g, &store, &c);
    let c = store.undo().unwrap();
    step(&mut g, &store, &c);
    assert_k2(&mut g, &store, "after move + undo");

    // 3. The removal undone (field 2 complete again), then redone.
    let c = store.undo().unwrap();
    step(&mut g, &store, &c);
    let c = store.redo().unwrap();
    step(&mut g, &store, &c);
    assert_k2(&mut g, &store, "after undo + redo of the removal");

    // 4. Reopening the file: the journal replays to the same project, same results.
    let path = store.path().to_path_buf();
    drop(store);
    let reopened = ProjectStore::open(&path).unwrap();
    let mut fresh = pv_pipeline::new_graph();
    sync_project(&mut fresh, reopened.project()).unwrap();
    assert_k2(&mut fresh, &reopened, "after reopening the file");
}
