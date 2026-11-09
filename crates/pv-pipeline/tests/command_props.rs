//! G4 property: over random sequences of *real* commands (`ProjectStore`: layout validation,
//! journal, undo / redo), the incrementally updated graph always equals a graph built from
//! scratch on the same project, node by node — and so do the compute thread's results.
//! E1: the sequences include entity commands (arrays re-laid out, edited, removed; roof
//! faces edited) and batches.

mod common;

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use common::*;
use pv_graph::{CancelToken, Instance};
use pv_model::{ArrayId, Command, ModuleId, ModuleSlot, ModuleState, Project};
use pv_pipeline::{
    sync_changes, sync_project, ComputeEvent, ComputeHandle, FieldOutcome, FieldStatics,
};
use pv_store::{Author, ProjectStore};

/// A random command on the current project, or undo / redo.
enum Op {
    Command(Command),
    Undo,
    Redo,
}

fn slots(p: &Project, a: ArrayId) -> Vec<ModuleSlot> {
    p.modules
        .iter()
        .filter(|m| m.array_id == a)
        .map(|m| ModuleSlot {
            row: m.row,
            col: m.col,
            u: m.u,
            v: m.v,
            state: m.state,
            string_index: m.string_index,
        })
        .collect()
}

/// E1 entity commands: structural re-layouts, entity edits and removals, batches.
fn random_entity_command(rng: &mut Rng, p: &Project) -> Option<Command> {
    let array = |rng: &mut Rng| -> Option<&pv_model::PanelArray> {
        (!p.arrays.is_empty()).then(|| &p.arrays[rng.below(p.arrays.len() as u64) as usize])
    };
    Some(match rng.below(6) {
        // Re-laid out with some modules gone for good (ids renumbered).
        0 | 1 => {
            let a = array(rng)?.id;
            Command::SetArrayModules {
                array_id: a,
                modules: slots(p, a)
                    .into_iter()
                    .filter(|_| rng.below(6) > 0)
                    .collect(),
            }
        }
        // A roof face slightly steeper (zones and loads change).
        2 => {
            let mut f = p.roof_faces[rng.below(p.roof_faces.len() as u64) as usize].clone();
            f.tilt_deg = (f.tilt_deg + 2.0).min(45.0);
            Command::UpsertRoofFace {
                face: Box::new(f),
                at: None,
            }
        }
        // An array unpaired / re-paired (east-west systems).
        3 => {
            let mut a = array(rng)?.clone();
            a.layout.paired_with = match a.layout.paired_with {
                Some(_) => None,
                None => p.arrays.iter().map(|o| o.id).find(|o| *o != a.id),
            };
            Command::UpsertArray {
                array: Box::new(a),
                at: None,
            }
        }
        4 => Command::RemoveArray { id: array(rng)?.id },
        // A batch: a re-layout and a discount, undone as one.
        _ => {
            let a = array(rng)?.id;
            let mut modules = slots(p, a);
            modules.truncate(modules.len() / 2);
            Command::Batch {
                commands: vec![
                    Command::SetArrayModules {
                        array_id: a,
                        modules,
                    },
                    Command::SetDiscount {
                        manufacturer: "K2 Systems".into(),
                        rate: Some(0.2),
                    },
                ],
            }
        }
    })
}

fn random_op(rng: &mut Rng, p: &Project) -> Op {
    let n = p.modules.len() as u64;
    if n == 0 {
        return Op::Undo;
    }
    let some = |rng: &mut Rng, k: u64| -> Vec<ModuleId> {
        let start = rng.below(n);
        (0..k).map(|j| ModuleId(((start + j) % n) as u32)).collect()
    };
    match rng.below(16) {
        // Small moves (stay in the gaps: mostly accepted, rarely change zones).
        0..=3 => {
            let k = 1 + rng.below(4);
            Op::Command(Command::MoveModules {
                ids: some(rng, k),
                du: rng.signed(0.004),
                dv: rng.signed(0.008),
            })
        }
        // A whole field moves: crosses zone boundaries (or is rejected near the edges).
        4 | 5 => {
            let a = p.arrays[rng.below(p.arrays.len() as u64) as usize].id;
            let ids = p
                .modules
                .iter()
                .filter(|m| m.array_id == a)
                .map(|m| m.id)
                .collect();
            Op::Command(Command::MoveModules {
                ids,
                du: rng.signed(0.6),
                dv: rng.signed(0.6),
            })
        }
        6 | 7 => {
            let k = 1 + rng.below(6);
            Op::Command(Command::RemoveModules { ids: some(rng, k) })
        }
        8 => {
            let ids: Vec<ModuleId> = p
                .modules
                .iter()
                .filter(|m| m.state == ModuleState::Removed)
                .map(|m| m.id)
                .take(1 + rng.below(4) as usize)
                .collect();
            if ids.is_empty() {
                Op::Undo
            } else {
                Op::Command(Command::RestoreModules { ids })
            }
        }
        9 | 10 => Op::Undo,
        13 | 14 => random_entity_command(rng, p).map_or(Op::Undo, Op::Command),
        // A discount (or its removal) on the K2 material.
        11 => Op::Command(Command::SetDiscount {
            manufacturer: "K2 Systems".into(),
            rate: (rng.below(3) > 0).then(|| rng.below(50) as f64 / 100.0),
        }),
        _ => Op::Redo,
    }
}

/// Applies `op`; `Some(changes)` when the project changed.
fn apply(store: &mut ProjectStore, op: &Op) -> Option<pv_model::ChangeSet> {
    let r = match op {
        Op::Command(c) => store.apply(c, Author::User),
        Op::Undo => store.undo(),
        Op::Redo => store.redo(),
    };
    r.ok().filter(|c| !c.is_empty())
}

fn run_sequences(base: &Project, name: &str, sequences: u64, steps: usize) -> (usize, usize) {
    let dir = tempfile::tempdir().unwrap();
    let (mut accepted, mut total) = (0, 0);
    // Recomputations of the zone loads after the first pass: proof that zone sets changed.
    let mut reloads = 0;
    for seed in 0..sequences {
        let mut rng = Rng(seed ^ 0xA5A5);
        let mut store = store_for(&dir, &format!("{name}-{seed}"), base);
        let mut g = pv_pipeline::new_graph();
        sync_project(&mut g, store.project()).unwrap();
        g.run_auto(&CancelToken::new()).unwrap();
        g.reset_stats();
        for step in 0..steps {
            let op = random_op(&mut rng, store.project());
            total += 1;
            if let Some(changes) = apply(&mut store, &op) {
                accepted += 1;
                sync_changes(&mut g, store.project(), &changes).unwrap();
            }
            // Passes: none (stale paths), a cancelled one, or a full one.
            match rng.below(4) {
                0 => {}
                1 => {
                    let t = CancelToken::new();
                    t.cancel();
                    let _ = g.run_auto(&t);
                }
                _ => {
                    g.run_auto(&CancelToken::new()).unwrap();
                }
            }
            if step % 5 == 4 {
                compare_with_full(&mut g, store.project())
                    .unwrap_or_else(|d| panic!("{name} seed {seed} step {step}:\n{d}"));
            }
        }
        compare_with_full(&mut g, store.project())
            .unwrap_or_else(|d| panic!("{name} seed {seed} end:\n{d}"));
        reloads += g.stats().computed("array.zone_loads");
    }
    eprintln!("{name}: zone loads recomputed {reloads} times after the first pass");
    assert!(reloads > sequences, "zone sets barely changed: {reloads}");
    (accepted, total)
}

#[test]
fn incremental_equals_full_over_real_command_sequences() {
    let (accepted, total) = run_sequences(&ref_a_roof1(), "ref-a", 200, 30);
    eprintln!("ref-a: {accepted}/{total} commands accepted");
    // The sequences must exercise the graph, not only rejected commands.
    assert!(accepted * 2 > total, "{accepted}/{total}");
}

#[test]
fn incremental_equals_full_on_an_east_west_system() {
    let (accepted, total) = run_sequences(&ref_b_east_west(), "ref-b", 25, 15);
    eprintln!("ref-b: {accepted}/{total} commands accepted");
    assert!(accepted * 3 > total, "{accepted}/{total}");
}

/// The same through the compute thread, with commands arriving while passes run: once the
/// thread is idle, the last report of every field equals a computation from scratch.
#[test]
fn compute_thread_results_equal_full_computation() {
    let dir = tempfile::tempdir().unwrap();
    let base = ref_a_roof1();
    for seed in 0..20u64 {
        let mut rng = Rng(seed ^ 0x5EED);
        let mut store = store_for(&dir, &format!("thread-{seed}"), &base);
        let (tx, rx) = mpsc::channel();
        let handle = ComputeHandle::spawn(move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        let mut seq = 1;
        handle.link().open(seq, store.project());
        for _ in 0..25 {
            let op = random_op(&mut rng, store.project());
            if let Some(changes) = apply(&mut store, &op) {
                seq += 1;
                handle.link().submit(seq, store.project(), &changes);
            }
            if rng.below(3) == 0 {
                std::thread::sleep(Duration::from_micros(rng.below(2000)));
            }
        }
        // Collect until the thread is idle on the last command.
        let mut latest: HashMap<ArrayId, (u32, FieldOutcome)> = HashMap::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(60)).expect("event") {
                ComputeEvent::Ready {
                    fields,
                    removed,
                    reset,
                    ..
                } => {
                    if reset {
                        latest.clear();
                    }
                    for a in removed {
                        latest.remove(&a);
                    }
                    for f in fields {
                        if latest.get(&f.array_id).is_none_or(|(s, _)| *s <= f.seq) {
                            latest.insert(f.array_id, (f.seq, f.outcome));
                        }
                    }
                }
                ComputeEvent::State {
                    busy: false,
                    seq: s,
                } if s == seq => break,
                ComputeEvent::Failed { error, .. } => panic!("seed {seed}: {error:?}"),
                _ => {}
            }
        }
        drop(handle);
        let mut full = pv_pipeline::new_graph();
        sync_project(&mut full, store.project()).unwrap();
        for a in &store.project().arrays {
            let expected = match full.get::<FieldStatics>(Instance::Array(a.id)) {
                Ok(r) => FieldOutcome::Ok {
                    result: (*r).clone(),
                },
                Err(e) => FieldOutcome::Failed {
                    error: pv_pipeline::report::command_error(&e),
                },
            };
            assert_eq!(
                latest.get(&a.id).map(|(_, o)| o),
                Some(&expected),
                "seed {seed} array {}",
                a.id.0
            );
        }
    }
}
